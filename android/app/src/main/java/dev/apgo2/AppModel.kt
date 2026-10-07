package dev.apgo2

import android.content.Context
import android.location.Location
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.util.UUID
import kotlin.math.cos
import kotlin.random.Random
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.ApEvent
import uniffi.apgo_ffi.ApSession
import uniffi.apgo_ffi.CircleOut
import uniffi.apgo_ffi.Engine
import uniffi.apgo_ffi.EventOut
import uniffi.apgo_ffi.GameInfo
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.HudOut
import uniffi.apgo_ffi.OfferOut
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.RealmOut
import uniffi.apgo_ffi.SoloOptionsIn
import uniffi.apgo_ffi.ZoneOut

class AppModel(private val ctx: Context, private val scope: CoroutineScope) {
    val engine = Engine(ctx.filesDir.absolutePath)

    var tab by mutableIntStateOf(0) // 0 Realms, 1 New Game, 2 Play
    var realms by mutableStateOf<List<RealmOut>>(emptyList())
    val offers = mutableStateMapOf<String, List<OfferOut>>()
    var busy by mutableStateOf<String?>(null)
    var status by mutableStateOf("")
    var quests by mutableStateOf<List<QuestOut>>(emptyList())
    var zones by mutableStateOf<List<ZoneOut>>(emptyList())
    var hud by mutableStateOf<HudOut?>(null)
    var games by mutableStateOf<List<GameInfo>>(emptyList())
    val log = mutableStateListOf<String>()
    var realLoc by mutableStateOf<Location?>(null)
    var simPos by mutableStateOf<LatLng?>(null)
    var drawing by mutableStateOf(false)
    val draft = mutableStateListOf<LatLng>()
    var selected by mutableStateOf<Long?>(null)
    var yamlText by mutableStateOf<String?>(null)

    // Archipelago
    var session by mutableStateOf<ApSession?>(null)
    var apStatus by mutableStateOf("not connected")
    var apSlotJson by mutableStateOf<String?>(null)
    var apZoneModes by mutableStateOf<List<String>>(emptyList())
    private var apSyncedChecked = false

    private var simClockMs = 0L

    fun now() = System.currentTimeMillis()

    val me: LatLng?
        get() = simPos ?: realLoc?.let { LatLng(it.latitude, it.longitude) }

    fun say(msg: String) {
        log.add(0, msg)
        while (log.size > 60) log.removeAt(log.lastIndex)
    }

    fun refreshAll() {
        realms = engine.realms()
        games = engine.games()
        realms.forEach { offers[it.id] = engine.realmOffers(it.id) }
        refreshPlay()
    }

    fun refreshPlay() {
        if (engine.hasGame()) {
            quests = engine.quests()
            zones = engine.zones()
            hud = engine.hud(now())
        } else {
            quests = emptyList(); zones = emptyList(); hud = null
        }
    }

    fun homePoint(): GeoPoint? = engine.home() ?: realms.firstOrNull()?.let { r -> r.circle?.center ?: r.polygon.firstOrNull() }

    // ----------------------------------------------------------------- realms
    fun saveDraftRealm(name: String, mode: String) {
        if (draft.size < 3) { status = "Tap at least 3 points on the map"; return }
        val id = UUID.randomUUID().toString()
        runCatching { engine.saveRealm(id, name.ifBlank { "Realm ${realms.size + 1}" }, mode, null, draft.map { GeoPoint(it.latitude, it.longitude) }) }
            .onSuccess { draft.clear(); drawing = false; refreshAll(); scan(id) }
            .onFailure { status = "Could not save: ${it.message}" }
    }

    fun saveCircleRealm(name: String, mode: String, radiusM: Double) {
        val c = me ?: run { status = "No location yet"; return }
        val id = UUID.randomUUID().toString()
        runCatching { engine.saveRealm(id, name.ifBlank { "Around me" }, mode, CircleOut(GeoPoint(c.latitude, c.longitude), radiusM), emptyList()) }
            .onSuccess { refreshAll(); scan(id) }
            .onFailure { status = "Could not save: ${it.message}" }
    }

    fun scan(id: String) {
        scope.launch {
            busy = "Scanning realm..."
            val result = withContext(Dispatchers.IO) { runCatching { engine.scanRealm(id, now().toULong()) } }
            busy = null
            result.onSuccess { offers[id] = it; status = "Scan done: ${it.size} quest kinds on offer" }
                .onFailure { status = "Scan failed: ${it.message}" }
            realms = engine.realms()
        }
    }

    fun deleteRealm(id: String) {
        runCatching { engine.deleteRealm(id) }
        refreshAll()
    }

    fun setHomeHere() {
        val c = me ?: run { status = "No location yet"; return }
        runCatching { engine.setHome(GeoPoint(c.latitude, c.longitude)) }
        status = "Home set to this spot"
    }

    // ------------------------------------------------------------------- games
    fun startSolo(opts: SoloOptionsIn, zoneRealms: List<String>, name: String) {
        scope.launch {
            busy = "Building your game..."
            val seed = Random.nextLong().toULong() shr 1
            val r = withContext(Dispatchers.IO) { runCatching { engine.startSolo(UUID.randomUUID().toString(), name, opts, zoneRealms, seed) } }
            busy = null
            r.onSuccess { simClockMs = 0; log.clear(); refreshAll(); tab = 2; status = "Game started!" }
                .onFailure { status = "Could not start: ${it.message}" }
        }
    }

    fun exportYaml(opts: SoloOptionsIn) {
        runCatching { engine.buildYaml("Player", opts) }.onSuccess { yamlText = it }.onFailure { status = "YAML failed: ${it.message}" }
    }

    fun openGame(id: String) {
        runCatching { engine.openGame(id) }.onSuccess { simClockMs = 0; refreshAll(); tab = 2 }.onFailure { status = "Could not open: ${it.message}" }
    }

    fun deleteGame(id: String) {
        runCatching { engine.deleteGame(id) }
        refreshAll()
    }

    // ------------------------------------------------------------------ events
    fun handle(events: List<EventOut>) {
        for (e in events) when (e) {
            is EventOut.QuestDone -> say("Done: ${e.name}")
            is EventOut.Reward -> say("Found: ${e.item}")
            is EventOut.ZoneUnlocked -> say("Zone ${e.zone} unlocked!")
            is EventOut.Trap -> say(e.message)
            is EventOut.Discovered -> {}
            is EventOut.GoalAchieved -> { say("GOAL ACHIEVED: ${e.label}"); status = "You won! ${e.label}" }
            is EventOut.Info -> say(e.text)
            is EventOut.SendCheck -> runCatching { session?.sendCheck(e.locationId) }.onFailure { say("check failed: ${it.message}") }
            is EventOut.ShuffleRequested -> runCatching { engine.reroll(emptyList(), Random.nextLong().toULong() shr 1) }
        }
    }

    fun onFix(loc: Location) {
        if (!engine.hasGame() || simPos != null) return
        handle(engine.onFix(loc.latitude, loc.longitude, now(), loc.accuracy.toDouble(), null))
        refreshPlay()
    }

    // ------------------------------------------------------------- dev simulator
    /** Virtual clock for the simulator: always moves forward, so dwell and trap timers behave. */
    private fun tick(ms: Long): Long {
        simClockMs = maxOf(simClockMs, now()) + ms
        return simClockMs
    }

    private var simSteps = 50_000L

    private fun fix(p: GeoPoint, advanceMs: Long, steps: Boolean = false): List<EventOut> {
        simPos = LatLng(p.lat, p.lon)
        if (steps) simSteps += 400
        return engine.onFix(p.lat, p.lon, tick(advanceMs), 5.0, if (steps) simSteps else null)
    }

    private fun offset(p: GeoPoint, northM: Double, eastM: Double) =
        GeoPoint(p.lat + northM / 111_195.0, p.lon + eastM / (111_195.0 * cos(Math.toRadians(p.lat))))

    /** Escape any trap the way a real player would (thaw point, detour waypoint, toll distance, leash). */
    private fun escapeTraps(home: GeoPoint, out: MutableList<EventOut>) {
        repeat(4) {
            val h = engine.hud(now()) ?: return
            when {
                h.thaw != null -> out += fix(h.thaw!!, 600_000)
                h.waypoint != null -> out += fix(h.waypoint!!, 600_000)
                h.blocked?.startsWith("Toll") == true -> { var p = home; repeat(8) { out += fix(p, 90_000); p = offset(p, 0.0, 160.0) } }
                h.blocked?.startsWith("Leash") == true -> out += fix(home, 600_000)
                else -> return
            }
        }
    }

    /** Feed the engine the kind of fix sequence a real player would produce to complete [q]. */
    fun devComplete(q: QuestOut) {
        scope.launch(Dispatchers.Default) {
            val a = q.anchor
            val home = homePoint() ?: a ?: return@launch
            val out = mutableListOf<EventOut>()
            escapeTraps(home, out)
            when (q.shape) {
                "point", "area" -> a?.let { out += fix(it, 600_000); out += fix(it, 400_000) }
                "dwell" -> a?.let { out += fix(it, 600_000); out += fix(it, 11 * 60_000L) }
                "courier" -> { q.anchor?.let { out += fix(it, 600_000) }; q.anchorB?.let { out += fix(it, limitMs(q) / 2) } }
                "roundtrip" -> { a?.let { out += fix(it, 600_000) }; out += fix(home, limitMs(q) / 2) }
                "line" -> {
                    out += fix(q.path.first(), 600_000)
                    for (i in 0 until q.path.size - 1) {
                        val p0 = q.path[i]; val p1 = q.path[i + 1]
                        val d = kotlin.math.hypot((p1.lat - p0.lat) * 111_195.0, (p1.lon - p0.lon) * 111_195.0 * cos(Math.toRadians(p0.lat)))
                        val n = kotlin.math.ceil(d / 10.0).toInt().coerceAtLeast(1)
                        for (k in 1..n) out += fix(GeoPoint(p0.lat + (p1.lat - p0.lat) * k / n, p0.lon + (p1.lon - p0.lon) * k / n), 15_000)
                    }
                }
                "cells" -> { var p = home; repeat(70) { out += fix(p, 90_000); p = offset(p, 0.0, 160.0) } } // ~6 km/h, like a walk
                "steps" -> repeat(30) { out += fix(home, 60_000, steps = true) }
                "away" -> {
                    val km = Regex("at least ([0-9.]+) km").find(q.detail)?.groupValues?.get(1)?.toDoubleOrNull() ?: 2.0
                    val mins = Regex("Spend (\\d+) min").find(q.detail)?.groupValues?.get(1)?.toIntOrNull() ?: 60
                    val far = offset(home, km * 1000 + 600, 0.0)
                    out += fix(far, 600_000)
                    repeat(mins / 4 + 3) { out += fix(far, 4 * 60_000L) } // the game only counts gaps up to 5 minutes
                }
            }
            android.util.Log.i("apgo", "sim ${q.name} shape=${q.shape} path=${q.path.size} state=${q.state} -> ${out.size} events ${out.take(4)}")
            withContext(Dispatchers.Main) { handle(out); refreshPlay() }
        }
    }

    /** The quest's own time limit ("within N min") in ms, so the simulator obeys it like a real player must. */
    private fun limitMs(q: QuestOut): Long = (Regex("within (\\d+) min").find(q.detail)?.groupValues?.get(1)?.toLongOrNull() ?: 20L) * 60_000L

    fun devTeleportNext() {
        val next = quests.firstOrNull { it.state == "open" || it.state == "progress" }
        if (next == null) { status = "No open quests (zones may be locked or all done)"; return }
        status = "Simulating: ${next.name}"
        devComplete(next)
    }

    // -------------------------------------------------------------- archipelago
    fun connectAp(url: String, slot: String) {
        apSyncedChecked = false
        apSlotJson = null
        session = ApSession.connect(url, slot, null, ctx.cacheDir.resolve("ap").absolutePath)
    }

    /** Called from a coroutine loop while a session exists. */
    suspend fun apTick() {
        val s = session ?: return
        val events = withContext(Dispatchers.IO) { runCatching { s.poll() }.getOrDefault(emptyList()) }
        events.forEach { e ->
            when (e) {
                is ApEvent.Connected -> {
                    apSlotJson = s.slotDataJson()
                    apZoneModes = apSlotJson?.let { runCatching { engine.slotZoneModes(it) }.getOrNull() } ?: emptyList()
                }
                is ApEvent.Error -> say("AP: ${e.detail}")
                is ApEvent.Print -> if (log.size < 40) say(e.text)
                else -> {}
            }
        }
        apStatus = s.status()
        if (engine.hasGame() && hud?.backend == "archipelago") {
            val items = withContext(Dispatchers.IO) { runCatching { s.receivedItems().map { it.name } }.getOrDefault(emptyList()) }
            if (!apSyncedChecked && apSlotJson != null) {
                engine.markChecked(s.checkedLocationIds(), now())
                apSyncedChecked = true
            }
            val pos = me?.let { GeoPoint(it.latitude, it.longitude) }
            handle(engine.syncItems(items, now(), pos))
            refreshPlay()
        }
    }

    fun startApGame(zoneRealms: List<String>, name: String) {
        val json = apSlotJson ?: run { status = "Connect first"; return }
        scope.launch {
            val seed = Random.nextLong().toULong() shr 1
            val r = withContext(Dispatchers.IO) { runCatching { engine.startArchipelago(UUID.randomUUID().toString(), name, json, "archipelago", zoneRealms, seed) } }
            r.onSuccess { simClockMs = 0; apSyncedChecked = false; refreshAll(); tab = 2; status = "Archipelago game started" }
                .onFailure { status = "Could not start: ${it.message}" }
        }
    }

    fun apGoalSummary(): String? = apSlotJson?.let { runCatching { JSONObject(it).optString("goal") }.getOrNull() }
}
