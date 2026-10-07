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
    /** The realm editor: null shows the realm list, "" a new realm, otherwise the id of the realm being edited. */
    var editing by mutableStateOf<String?>(null)
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
    var home by mutableStateOf<GeoPoint?>(null)
    val draft = mutableStateListOf<LatLng>()
    var selected by mutableStateOf<Long?>(null)
    var yamlText by mutableStateOf<String?>(null)
    /** Cumulative steps since boot from the phone's step counter (null when unavailable or not permitted). */
    var stepsTotal by mutableStateOf<Long?>(null)
    /** any | prefer_paved | paved_only */
    var surfacePref by mutableStateOf("any")
    var avoidStairs by mutableStateOf(false)

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
        home = engine.home()
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

    fun homePoint(): GeoPoint? = engine.home() ?: realms.firstOrNull()?.let { r -> if (r.polygonActive) r.polygon.firstOrNull() else r.circle?.center }

    // ----------------------------------------------------------------- realms
    /** "Realm N" with the lowest N not already used by a realm. */
    fun defaultRealmName(): String {
        val taken = realms.map { it.name }.toSet()
        return generateSequence(1) { it + 1 }.map { "Realm $it" }.first { it !in taken }
    }

    /**
     * Creates (id == null) or updates a realm and returns its id, or null (with a status message) when it could not be saved. Both outlines are kept;
     * [polygonActive] picks the real one. A blank name becomes "Realm N". Nothing is scanned here: that is the caller's decision.
     */
    fun saveRealm(id: String?, name: String, icon: String?, circle: Pair<LatLng, Double>?, polygon: List<LatLng>, polygonActive: Boolean): String? {
        if (polygonActive && polygon.size < 3) return null
        if (!polygonActive && circle == null) return null
        val rid = id ?: UUID.randomUUID().toString()
        val c = circle?.let { (p, r) -> CircleOut(GeoPoint(p.latitude, p.longitude), r) }
        return runCatching {
            engine.saveRealm(rid, name.ifBlank { defaultRealmName() }, icon, c, polygon.map { GeoPoint(it.latitude, it.longitude) }, polygonActive)
        }
            .onSuccess { realms = engine.realms() }
            .onFailure { status = "Could not save: ${it.message}" }
            .map { rid }
            .getOrNull()
    }

    /** A scan that needs a lot of downloading and waits for the player's go-ahead. */
    data class ScanAsk(val id: String, val requests: Int, val tiles: Int)

    var scanAsk by mutableStateOf<ScanAsk?>(null)
    /** 0..1 while a scan runs; null when it has no measurable progress. */
    var busyFraction by mutableStateOf<Float?>(null)
    private val scanning = mutableSetOf<String>()
    private val waitingToScan = mutableSetOf<String>()
    private val lastDownload = mutableMapOf<String, Long>()
    private val quietRetries = mutableMapOf<String, Int>()

    /**
     * Fetch the finds of a realm. Anything already downloaded (by this or any other realm) is reused, so a repeated or overlapping scan costs nothing.
     * Downloads are rationed: at most one per realm every [SCAN_COOLDOWN_MS] (a request inside the window is deferred, not lost), and a big area asks
     * first. Trouble reaching the map servers is retried quietly in the background.
     */
    fun scan(id: String, confirmed: Boolean = false) {
        scope.launch {
            if (id in scanning) return@launch
            val plan = withContext(Dispatchers.IO) { engine.scanPlan(id) }
            if (plan.missing > 0u) {
                val wait = SCAN_COOLDOWN_MS - (now() - (lastDownload[id] ?: 0L))
                if (wait > 0) {
                    if (waitingToScan.add(id)) scope.launch { delay(wait); waitingToScan.remove(id); scan(id, confirmed) }
                    return@launch
                }
                if (plan.missing >= BIG_SCAN_REQUESTS.toUInt() && !confirmed) {
                    scanAsk = ScanAsk(id, plan.missing.toInt(), plan.tiles.toInt())
                    return@launch
                }
            }
            scanning.add(id)
            busy = "Looking for finds…"
            busyFraction = null
            val progress = scope.launch {
                while (true) {
                    val p = engine.scanProgress()
                    if (p.total > 0u) { busyFraction = p.done.toFloat() / p.total.toFloat(); busy = "Looking for finds… ${p.done} of ${p.total}" }
                    delay(400)
                }
            }
            val result = withContext(Dispatchers.IO) { runCatching { engine.scanRealm(id, now().toULong()) } }
            progress.cancel()
            scanning.remove(id)
            busy = null
            busyFraction = null
            if (plan.missing > 0u) lastDownload[id] = now()
            realms = engine.realms()
            result.onSuccess { offers[id] = it; status = "" }
                .onFailure { status = "Couldn't reach the map servers just now. Trying again in a moment." }
            // Pieces that did not arrive (or a failure) are picked up again later, a few times, without bothering the player.
            val partial = result.isFailure || realms.firstOrNull { it.id == id }?.warning != null
            if (partial && (quietRetries[id] ?: 0) < MAX_QUIET_RETRIES) {
                quietRetries[id] = (quietRetries[id] ?: 0) + 1
                scope.launch { delay(QUIET_RETRY_MS); scan(id, confirmed = true) }
            } else if (!partial) {
                quietRetries.remove(id)
            }
        }
    }

    /** Favorite or ban a scanned place ("none" clears it). Offers update now; quests change the next time they are made or re-rolled. */
    fun setFindMark(realmId: String, placeId: String, mark: String): Boolean =
        runCatching { engine.setFindMark(realmId, placeId, mark) }
            .onSuccess { offers[realmId] = engine.realmOffers(realmId) }
            .onFailure { status = "Could not save: ${it.message}" }
            .isSuccess

    /** A realm that was just deleted and can still be brought back. */
    data class UndoDelete(val id: String, val name: String)

    var undo by mutableStateOf<UndoDelete?>(null)
    private val hidden = mutableStateListOf<String>()

    /** The realms to show: those waiting out their undo window are hidden but not yet deleted. */
    val shownRealms: List<RealmOut> get() = realms.filter { it.id !in hidden }

    /** Hide a realm and offer Undo; it is really deleted when [commitDelete] runs (after the undo bar goes away). */
    fun deleteWithUndo(id: String) {
        val r = realms.firstOrNull { it.id == id } ?: return
        undo?.let { commitDelete(it.id) } // a new delete settles the previous one
        hidden.add(id)
        undo = UndoDelete(id, r.name)
    }

    fun undoDelete(id: String) {
        hidden.remove(id)
        if (undo?.id == id) undo = null
    }

    fun commitDelete(id: String) {
        if (id !in hidden) return
        runCatching { engine.deleteRealm(id) }
        hidden.remove(id)
        if (undo?.id == id) undo = null
        refreshAll()
    }

    fun setHomeHere() {
        val c = me ?: run { status = "No location yet"; return }
        setHome(c)
    }

    fun setHome(p: LatLng) {
        runCatching { engine.setHome(GeoPoint(p.latitude, p.longitude)) }
            .onSuccess { home = engine.home(); status = "Home set" }
            .onFailure { status = "Could not set home: ${it.message}" }
    }

    // ------------------------------------------------------------------- games
    fun startSolo(opts: SoloOptionsIn, zoneRealms: List<String>, name: String) {
        scope.launch {
            busy = "Building your game..."
            val seed = Random.nextLong().toULong() shr 1
            val r = withContext(Dispatchers.IO) { runCatching { engine.startSolo(UUID.randomUUID().toString(), name, opts, zoneRealms, seed, surfacePref, avoidStairs) } }
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
            is EventOut.GoalAchieved -> {
                say("GOAL ACHIEVED: ${e.label}"); status = "You won! ${e.label}"
                if (hud?.backend == "archipelago") runCatching { session?.sendGoal() }.onFailure { say("could not report goal: ${it.message}") }
            }
            is EventOut.Info -> say(e.text)
            is EventOut.SendCheck -> runCatching { session?.sendCheck(e.locationId) }.onFailure { say("check failed: ${it.message}") }
            is EventOut.ShuffleRequested -> runCatching { engine.reroll(emptyList(), Random.nextLong().toULong() shr 1) }
        }
    }

    fun onFix(loc: Location) {
        if (!engine.hasGame() || simPos != null) return
        handle(engine.onFix(loc.latitude, loc.longitude, now(), loc.accuracy.toDouble(), stepsTotal))
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
        // Fog: undiscovered quests are not "open", but a walker heading for one discovers it on arrival.
        val next = quests.firstOrNull { it.state == "open" || it.state == "progress" } ?: quests.firstOrNull { it.state == "hidden" }
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
            val r = withContext(Dispatchers.IO) { runCatching { engine.startArchipelago(UUID.randomUUID().toString(), name, json, "archipelago", zoneRealms, seed, surfacePref, avoidStairs) } }
            r.onSuccess { simClockMs = 0; apSyncedChecked = false; refreshAll(); tab = 2; status = "Archipelago game started" }
                .onFailure { status = "Could not start: ${it.message}" }
        }
    }

    fun apGoalSummary(): String? = apSlotJson?.let { runCatching { JSONObject(it).optString("goal") }.getOrNull() }
}

/** At most one download per realm in this time. */
private const val SCAN_COOLDOWN_MS = 20_000L

/** A scan needing this many downloads asks first (about 20 tiles). */
private const val BIG_SCAN_REQUESTS = 60

private const val QUIET_RETRY_MS = 45_000L
private const val MAX_QUIET_RETRIES = 4
