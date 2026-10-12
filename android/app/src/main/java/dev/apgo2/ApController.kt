package dev.apgo2

import android.content.Context
import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONObject
import uniffi.apgo_ffi.ApEvent
import uniffi.apgo_ffi.ApPoll
import uniffi.apgo_ffi.ApReconnect
import uniffi.apgo_ffi.ApSession
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.apNeedsSync
import java.util.UUID
import kotlin.random.Random

private const val MAX_PRINT_LOG = 40

// A LAN server blocked by Android 17's local network protection never answers: after this long, show LAN_HINT.
private const val LAN_HINT_AFTER_MS = 10_000L

// The longest a quiet, connected session sleeps before looking again (a safety net: the server's own pings wake it sooner).
private const val AP_IDLE_WAIT_MS = 60_000

// How long a probe after a silent wait may take to be answered before the connection counts as dead.
private const val AP_PROBE_WAIT_MS = 15_000

// The core's ApSession.status() while connected.
private const val CONNECTED = "connected"

// Silence after which a connected session counts as dead when the app comes back: the stock server pings every 20 s.
private const val AP_STALE_MS = 45_000L

/** How the poll loop waits between drains. */
internal object ApWait {
    /** Whether to sleep until the server sends something: connected and quiet. Otherwise the core's back-off times the next drain. */
    fun sleeps(
        status: String,
        active: Boolean,
    ): Boolean = status == CONNECTED && !active

    /**
     * Whether a whole wait passed with no data: the Archipelago server pings every 20 s, so silence for [AP_IDLE_WAIT_MS] is a socket
     * that died without telling (a network switch, a NAT timeout). Reconnecting makes the server send every item again.
     */
    fun dead(
        slept: Boolean,
        woke: Boolean,
    ): Boolean = slept && !woke

    /**
     * Whether to reconnect a session that dropped (a hard Archipelago client requirement: reconnect if the connection is lost while
     * playing). Only after it was connected to this server: a first connect that fails (wrong slot, password, address) is the
     * player's to fix, not a loop.
     */
    fun reconnects(
        status: String,
        everConnected: Boolean,
    ): Boolean = everConnected && status.startsWith("disconnected")

    /**
     * Whether the app, coming to the front, reconnects at once: the session dropped, or it shows connected but the server was
     * silent for [silentMs] (elapsed real time, which runs on while the phone sleeps and the wait's timer cannot), longer than its
     * 20 s pings allow.
     */
    fun stale(
        status: String,
        everConnected: Boolean,
        silentMs: Long,
    ): Boolean = reconnects(status, everConnected) || (status == CONNECTED && silentMs > AP_STALE_MS)
}

/** The connection to an Archipelago server and the games played over it. */
internal class ApController(
    private val model: AppModel,
    private val ctx: Context,
    private val scope: CoroutineScope,
) {
    private val sessions = SessionSlot<ApSession> { it.close() }
    val session get() = sessions.current
    var status by mutableStateOf("not connected")

    // A line shown next to [status]; kept apart because tick() rewrites status from the session on every poll.
    var hint by mutableStateOf<String?>(null)
    var slotJson by mutableStateOf<String?>(null)
    var zoneModes by mutableStateOf<List<String>>(emptyList())
    private var syncedChecked = false
    private var pollJob: Job? = null

    // Where the session connected, to reconnect a connection that dropped or died silently; whether it ever connected there; and
    // when to try again (5 s doubling, capped, in the core).
    private var lastTarget: Pair<String, String>? = null
    private var everConnected = false
    private val reconnect = ApReconnect()

    // One drain at a time (the loop and a foreground or game-open sync), and when the server last sent anything (elapsedRealtime).
    private val tickLock = Mutex()
    private var lastDataMs = SystemClock.elapsedRealtime()

    // The game last synced with this session (its id), so a newly opened or started game syncs at once.
    private var syncedFor: String? = null

    /** Open a session to [url] as [slot]; [auto] is a reconnect of the same target (keeps its back-off). */
    fun connect(
        url: String,
        slot: String,
        auto: Boolean = false,
    ) {
        if (!auto) {
            everConnected = false
            reconnect.reset()
        }
        syncedChecked = false
        syncedFor = null
        slotJson = null
        hint = null
        lastTarget = url to slot
        val s = ApSession.connect(url, slot, null, ctx.cacheDir.resolve("ap").absolutePath)
        sessions.replace(s)
        // Polled from the model's scope, not the screen: items keep arriving while the app tracks in the background. Ends by itself
        // when this session is replaced.
        pollJob?.cancel()
        pollJob = scope.launch { runSession(s) }
        scope.launch {
            delay(LAN_HINT_AFTER_MS)
            if (session === s && s.status() == "connecting" && ctx.lacksLocalNetwork()) hint = LAN_HINT
        }
    }

    // The session's loop, until it is replaced: drain, then sleep in the socket until the server sends something (not a timer);
    // reconnect when it drops (after a back-off) or a whole wait passes in silence.
    private suspend fun runSession(s: ApSession) {
        val poll = ApPoll() // the back-off while connecting lives in the core
        while (session === s) {
            val active = tick()
            if (status == CONNECTED && !everConnected) {
                everConnected = true
                reconnect.reset()
            }
            if (ApWait.reconnects(status, everConnected)) {
                delay(reconnect.nextDelayMs().toLong())
                return reconnectFrom(s)
            }
            val slept = ApWait.sleeps(status, active)
            // A session closed by a reconnect meanwhile throws; that is a wake like any other (the loop then ends).
            val woke = slept && withContext(Dispatchers.IO) { runCatching { s.waitForData(AP_IDLE_WAIT_MS.toUInt()) }.getOrDefault(true) }
            if (active || woke) lastDataMs = SystemClock.elapsedRealtime()
            if (ApWait.dead(slept, woke) && !probeAnswered(s)) {
                Diag.warn("ap", "silent connection: reconnecting", "waited_ms" to AP_IDLE_WAIT_MS)
                return reconnectFrom(s)
            }
            delay(poll.nextDelayMs(active || woke).toLong()) // at least the short wait: a closed socket stays readable
        }
    }

    // After a whole silent wait: ask the server something it always answers. A live connection on a server that does not ping
    // wakes and stays (no reconnect, which would announce a leave and join to the room); a dead one stays silent.
    private suspend fun probeAnswered(s: ApSession): Boolean {
        val answered =
            withContext(Dispatchers.IO) {
                runCatching {
                    s.probe()
                    s.waitForData(AP_PROBE_WAIT_MS.toUInt())
                }.getOrDefault(false)
            }
        if (answered) lastDataMs = SystemClock.elapsedRealtime()
        return answered
    }

    // A new session to the same server, unless the player connected elsewhere meanwhile; it replaces [s]'s loop.
    private fun reconnectFrom(s: ApSession) {
        if (session === s) lastTarget?.let { (url, slot) -> connect(url, slot, auto = true) }
    }

    /**
     * The app came to the front: reconnect at once a session that dropped or went silent while the phone slept ([ApWait.stale]),
     * else drain and sync now rather than at the next server message (the player is looking).
     */
    fun onForeground() {
        val s = session ?: return
        if (ApWait.stale(status, everConnected, SystemClock.elapsedRealtime() - lastDataMs)) {
            Diag.info("ap", "stale on foreground: reconnecting", "was" to status)
            reconnect.reset()
            reconnectFrom(s)
        } else {
            syncNow()
        }
    }

    /** Drain and sync now (a game was started, opened or resumed: it shows its items at once, not at the next server message). */
    fun syncNow() {
        if (session != null) scope.launch { tick() }
    }

    /**
     * Drain the session (from its loop, [syncNow] or [onForeground], one at a time); true when the server sent anything. The open
     * game is synced only when items or server data changed, or it was never synced with this session.
     */
    suspend fun tick(): Boolean = tickLock.withLock { drain() }

    private suspend fun drain(): Boolean {
        var active = false
        sessions.use { s ->
            val events = withContext(Dispatchers.IO) { runCatching { s.poll() }.getOrDefault(emptyList()) }
            if (session !== s) return false // reconnected mid-poll: these events belong to the old server
            active = events.isNotEmpty()
            events.forEach { handle(s, it) }
            s.status().let {
                if (it != status) Diag.info("ap", "status", "status" to it)
                status = it
            }
            // The open Archipelago game, from the engine's memory; none (no game, a solo game, or paused) forgets the last sync, so a
            // game reopened after a pause catches up at once.
            val game = model.engine.openGameId()?.takeIf { model.hud?.backend == "archipelago" }
            if (game == null) syncedFor = null
            val changed = events.any { it is ApEvent.Connected || it is ApEvent.ReceivedItems || it is ApEvent.Updated }
            if (apNeedsSync(changed, syncedFor, game)) {
                syncGame(s)
                syncedFor = game
            }
        }
        return active
    }

    /** Start a game from the connected slot's data. */
    fun startGame(
        zoneRealms: List<String>,
        name: String,
    ) {
        val json = slotJson
        if (json == null) {
            model.status = "Connect first"
            return
        }
        scope.launch {
            val seed = Random.nextLong().toULong() shr 1
            val r =
                withContext(Dispatchers.IO) {
                    runCatching {
                        model.engine.startArchipelago(
                            UUID.randomUUID().toString(),
                            name,
                            json,
                            "archipelago",
                            zoneRealms,
                            seed,
                            model.surfacePref,
                            model.avoidStairs,
                            model.allowParking,
                        )
                        model.stepCal.loadInto(model.engine) // as a solo start does: the saved calibration, not a fresh one
                    }
                }
            r.onSuccess {
                model.library.refreshStreets(null)
                model.sim.resetClock()
                syncedChecked = false
                model.refreshAll()
                model.nav.show(AppTab.PLAY)
                model.status = "Archipelago game started"
            }
            r.onFailure { model.fail("start_game", "Could not start", it) }
        }
    }

    /** The goals of the connected game in words ("Letter Hunt, The Big One (any one)"), from slot_data v3 (or the single goal of v2). */
    fun goalSummary(): String? =
        slotJson?.let { json ->
            runCatching {
                val o = JSONObject(json)
                val goals = o.optJSONArray("goals")
                if (goals == null) {
                    o.optString("goal").ifBlank { null }
                } else {
                    val names = (0 until goals.length()).map { goals.getJSONObject(it).getString("id").replace('_', ' ') }
                    if (names.size == 1) names[0] else "${names.joinToString(", ")} (${goalRule(o)})"
                }
            }.getOrNull()
        }

    private fun goalRule(o: JSONObject) =
        when (o.optString("goal_requirement")) {
            "all" -> "all"
            "at_least" -> "at least ${o.optInt("goal_need")}"
            else -> "any one"
        }

    private fun handle(
        s: ApSession,
        e: ApEvent,
    ) {
        when (e) {
            is ApEvent.Connected -> {
                slotJson = s.slotDataJson()
                zoneModes = slotJson?.let { runCatching { model.engine.slotZoneModes(it) }.getOrNull() } ?: emptyList()
            }

            is ApEvent.Error -> {
                model.say("AP: ${e.detail}")
            }

            is ApEvent.Print -> {
                if (model.log.size < MAX_PRINT_LOG) model.say(e.text)
            }

            else -> {}
        }
    }

    // First sync after every (re)connect: the protocol asks clients to send checks made while offline on connection, and the
    // server ignores repeats, so send everything finished (one packet) and the goal if it was reached.
    private suspend fun reportProgress(s: ApSession) {
        val report = model.engine.serverReport() ?: return
        withContext(Dispatchers.IO) {
            if (report.checks.isNotEmpty()) {
                runCatching {
                    s.sendChecks(
                        report.checks,
                    )
                }.onFailure { Diag.warn("ap", "re-sending checks failed", "error" to it.toString()) }
            }
            if (report.goal) runCatching { s.sendGoal() }.onFailure { Diag.warn("ap", "re-sending goal failed", "error" to it.toString()) }
        }
        Diag.info("ap", "progress sent", "checks" to report.checks.size, "goal" to report.goal)
    }

    private suspend fun syncGame(s: ApSession) {
        val items = withContext(Dispatchers.IO) { runCatching { s.receivedItems().map { it.name } }.getOrDefault(emptyList()) }
        if (session !== s) return // reconnected meanwhile: the old server's items and checks are not this game's
        if (!syncedChecked && slotJson != null) {
            model.engine.markChecked(s.checkedLocationIds(), model.now())
            syncedChecked = true
            reportProgress(s)
        }
        val pos = model.acceptedHere?.let { GeoPoint(it.latitude, it.longitude) }
        model.handle(model.engine.syncItems(items, model.now(), pos))
        model.refreshPlay(withTrace = false) // items never change where you walked
    }
}
