package dev.apgo2

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import uniffi.apgo_ffi.ApEvent
import uniffi.apgo_ffi.ApSession
import uniffi.apgo_ffi.GeoPoint
import java.util.UUID
import kotlin.random.Random

private const val MAX_PRINT_LOG = 40

// A LAN server blocked by Android 17's local network protection never answers: after this long, show LAN_HINT.
private const val LAN_HINT_AFTER_MS = 10_000L

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

    /** Open a session to [url] as [slot]. */
    fun connect(
        url: String,
        slot: String,
    ) {
        syncedChecked = false
        slotJson = null
        hint = null
        val s = ApSession.connect(url, slot, null, ctx.cacheDir.resolve("ap").absolutePath)
        sessions.replace(s)
        scope.launch {
            delay(LAN_HINT_AFTER_MS)
            if (session === s && s.status() == "connecting" && ctx.lacksLocalNetwork()) hint = LAN_HINT
        }
    }

    /** Called from a coroutine loop while a session exists. */
    suspend fun tick() {
        sessions.use { s ->
            val events = withContext(Dispatchers.IO) { runCatching { s.poll() }.getOrDefault(emptyList()) }
            if (session !== s) return // reconnected mid-poll: these events belong to the old server
            events.forEach { handle(s, it) }
            s.status().let {
                if (it != status) Diag.info("ap", "status", "status" to it)
                status = it
            }
            if (model.engine.hasGame() && model.hud?.backend == "archipelago") syncGame(s)
        }
    }

    /** Start a game from the connected slot's data. */
    fun startGame(
        zoneRealms: List<String>,
        name: String,
        awayZoneOnly: Boolean,
        awayDistanceM: UInt,
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
                            awayZoneOnly,
                            awayDistanceM,
                        )
                    }
                }
            r.onSuccess {
                model.sim.resetClock()
                syncedChecked = false
                model.refreshAll()
                model.tab = AppTab.PLAY
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

    private suspend fun syncGame(s: ApSession) {
        val items = withContext(Dispatchers.IO) { runCatching { s.receivedItems().map { it.name } }.getOrDefault(emptyList()) }
        if (!syncedChecked && slotJson != null) {
            model.engine.markChecked(s.checkedLocationIds(), model.now())
            syncedChecked = true
        }
        val pos = model.me?.let { GeoPoint(it.latitude, it.longitude) }
        model.handle(model.engine.syncItems(items, model.now(), pos))
        model.refreshPlay()
    }
}
