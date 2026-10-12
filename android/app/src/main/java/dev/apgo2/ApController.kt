package dev.apgo2

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
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

/** How the poll loop waits between drains. */
internal object ApWait {
    /** Whether to sleep until the server sends something: connected and quiet. Otherwise the core's back-off times the next drain. */
    fun sleeps(
        status: String,
        active: Boolean,
    ): Boolean = status == "connected" && !active

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
            if (status == "connected" && !everConnected) {
                everConnected = true
                reconnect.reset()
            }
            if (ApWait.reconnects(status, everConnected)) {
                delay(reconnect.nextDelayMs().toLong())
                return reconnectFrom(s)
            }
            val slept = ApWait.sleeps(status, active)
            val woke = slept && withContext(Dispatchers.IO) { s.waitForData(AP_IDLE_WAIT_MS.toUInt()) }
            if (ApWait.dead(slept, woke)) {
                Diag.warn("ap", "silent connection: reconnecting", "waited_ms" to AP_IDLE_WAIT_MS)
                return reconnectFrom(s)
            }
            delay(poll.nextDelayMs(active || woke).toLong()) // at least the short wait: a closed socket stays readable
        }
    }

    // A new session to the same server, unless the player connected elsewhere meanwhile; it replaces [s]'s loop.
    private fun reconnectFrom(s: ApSession) {
        if (session === s) lastTarget?.let { (url, slot) -> connect(url, slot, auto = true) }
    }

    /** The app came to the front: drain and sync now rather than at the next server message (the player is looking). */
    fun onForeground() {
        if (session != null) scope.launch { tick() }
    }

    /**
     * Called from the poll loop started in [connect]; true when the server sent anything (the loop then polls again soon, see
     * [ApPoll]). The open game is synced only when items or server data changed, or it was never synced with this session.
     */
    suspend fun tick(): Boolean {
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
