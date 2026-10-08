package dev.apgo2

import android.content.Context
import android.location.Location
import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.presence.Debouncer
import dev.apgo2.presence.Decision
import dev.apgo2.presence.GpsMode
import dev.apgo2.presence.PresenceMonitor
import dev.apgo2.presence.PresencePolicy
import dev.apgo2.presence.PresenceSeeding
import dev.apgo2.presence.PresenceSettings
import dev.apgo2.presence.PresenceSignals
import dev.apgo2.presence.PresenceState
import dev.apgo2.presence.SetupProgress
import dev.apgo2.presence.SetupStep
import dev.apgo2.presence.Signals
import dev.apgo2.presence.Zone
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
import uniffi.apgo_ffi.AuditEventOut
import uniffi.apgo_ffi.AwayReportOut
import uniffi.apgo_ffi.ChainOut
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

private const val AWAY_MIN_MS = 60_000L
private const val REEVALUATE_MS = 5_000L
private const val SEED_TIMEOUT_MS = 3_000L
private const val SEED_POLL_MS = 1_000L

class AppModel(private val ctx: Context, private val scope: CoroutineScope) {
    val engine = Engine(ctx.filesDir.absolutePath)
    val sensors = Sensors(ctx, this)
    val settings = PresenceSettings(ctx)
    val monitor = PresenceMonitor(ctx) { onMonitorChange() }
    /** What the presence rules decided last (the status chip shows its state). */
    var presence by mutableStateOf(Decision(PresenceState.Stopped, GpsMode.Off, counting = false))
        private set
    private val homeDebounce = Debouncer()
    private val carDebounce = Debouncer()
    private var zone = Zone.Unknown
    /** Reloading the whole trace on every fix gets slower as it grows; every 10 s is plenty for a line on a map. */
    private val traceThrottle = Throttle(10_000)
    /** Step readings without events only refresh the Play screen this often. */
    private val stepRefreshThrottle = Throttle(5_000)

    var tab by mutableIntStateOf(0) // 0 Realms, 1 New Game, 2 Play
    /** The realm editor: null shows the realm list, "" a new realm, otherwise the id of the realm being edited. */
    var editing by mutableStateOf<String?>(null)
    /** The setup wizard (home pin, home Wi-Fi, car Bluetooth) is open. It opens by itself on each app (process) start until it has been finished or skipped once. */
    var showSetup by mutableStateOf(!settings.setupDone)
        private set
    /** The step the wizard opens on. */
    var setupStart = SetupStep.Home
        private set

    fun setupProgress() = SetupProgress(home != null, settings.homeNetworks.size, settings.carDevices.size, settings.setupDone)
    fun openSetup(from: SetupStep? = null) { setupStart = from ?: SetupStep.Home; showSetup = true }
    /** Close without marking it done (Back on the first step): the Home card keeps offering it. */
    fun leaveSetup() { showSetup = false }
    fun finishSetup() { settings.setupDone = true; showSetup = false }
    var realms by mutableStateOf<List<RealmOut>>(emptyList())
    val offers = mutableStateMapOf<String, List<OfferOut>>()
    var busy by mutableStateOf<String?>(null)
    var status by mutableStateOf("")
    var quests by mutableStateOf<List<QuestOut>>(emptyList())
    var zones by mutableStateOf<List<ZoneOut>>(emptyList())
    var hud by mutableStateOf<HudOut?>(null)
    /** Where you have been in this game: one line per unbroken stretch of GPS. */
    var trace by mutableStateOf<List<List<LatLng>>>(emptyList())
    /** Set when you come back to the app after being away; shown once. */
    var away by mutableStateOf<AwayReportOut?>(null)
    var games by mutableStateOf<List<GameInfo>>(emptyList())
    /** What happened in the open (or last paused) game, newest first; see [refreshActivity]. */
    var activity by mutableStateOf<List<AuditEventOut>>(emptyList())
    val log = mutableStateListOf<String>()
    var realLoc by mutableStateOf<Location?>(null)
    var simPos by mutableStateOf<LatLng?>(null)
    var home by mutableStateOf<GeoPoint?>(null)
    val draft = mutableStateListOf<LatLng>()
    var selected by mutableStateOf<Long?>(null)
    var chains by mutableStateOf<List<ChainOut>>(emptyList())
    var selectedChain by mutableStateOf<String?>(null)
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

    private fun fail(what: String, t: Throwable) = Diag.e("model", "$what failed", t)

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
        evaluatePresence() // the `playing` signal follows `hud`, which every game start, open and pause goes through here
    }

    fun refreshPlay(withTrace: Boolean = true) {
        if (engine.hasGame()) {
            quests = engine.quests()
            zones = engine.zones()
            chains = engine.chains()
            hud = engine.hud(now())
            if (withTrace) trace = engine.track(0L, Long.MAX_VALUE).map { seg -> seg.points.map { LatLng(it.lat, it.lon) } }
        } else {
            quests = emptyList(); zones = emptyList(); chains = emptyList(); selectedChain = null; hud = null; trace = emptyList()
        }
    }

    /** The phone's step counter changed: credit it to the open game (the engine ignores it when no game is open). */
    fun onSteps(total: Long) {
        stepsTotal = total
        if (!engine.hasGame()) return
        val events = engine.onSteps(total, now())
        handle(events)
        // The counter reports about twice a second: refresh the screen when something happened, otherwise only now and then.
        if (events.isNotEmpty() || stepRefreshThrottle.due(now())) refreshPlay(withTrace = false)
    }

    /** Log quest progress each time it crosses a 10% step, so a quest that never moves shows up in the log. */
    private fun logProgress() {
        for (q in quests) {
            if (q.state != "progress") { progressBuckets.remove(q.locationId); continue }
            val b = (q.progress * 10).toInt()
            if (progressBuckets.put(q.locationId, b) != b) Diag.i("progress", q.name, "kind" to q.kindId, "percent" to b * 10, "id" to q.locationId)
        }
    }

    private val main = android.os.Handler(android.os.Looper.getMainLooper())
    private val reevaluate = Runnable { evaluatePresence() }
    private val seedPoll = Runnable { pollSeeding() }
    private val seeding = PresenceSeeding(SEED_TIMEOUT_MS)
    private var monitorBluetooth: Boolean? = null
    var locationPermitted by mutableStateOf(false)

    private fun rawHome() = PresenceSignals.isHome(monitor.currentWifi, settings.homeNetworks)
    private fun rawCar() = PresenceSignals.carConnected(monitor.connectedCarCandidates, settings.carDevices)

    /**
     * Each debouncer takes its first real reading once per monitor start, on its own signal (see [PresenceSeeding]); a signal that
     * never answers (not on Wi-Fi, Bluetooth off) is read as it stands after the timeout. This one runnable reschedules itself while
     * seeding is incomplete and is never cancelled by monitor events, which only call [pollSeeding] too.
     * Known limit: after a process start the first game open waits for seeding (up to 3 s) with the previous decision (Stopped:
     * not counting, idle GPS rate).
     */
    private fun pollSeeding() {
        val plan = seeding.poll(SystemClock.elapsedRealtime(), monitor.wifiReported, monitor.bluetoothReady)
        if (plan.home) homeDebounce.seed(rawHome())
        if (plan.car) carDebounce.seed(rawCar())
        main.removeCallbacks(seedPoll)
        if (seeding.waiting) main.postDelayed(seedPoll, SEED_POLL_MS)
        evaluatePresence()
    }

    private fun onMonitorChange() = pollSeeding()

    /**
     * Start the Wi-Fi/Bluetooth watcher once location permission is there, and again only when the Bluetooth grant changes.
     * It is never stopped within the process life: it belongs to the model (like the sensors), not to the activity, so rotation or
     * swiping the app away cannot leave the presence state stale while the tracking service keeps the game running. It is cheap
     * (two registered callbacks). A failed start is not recorded, so the next call retries.
     */
    fun ensureMonitor(bluetoothGranted: Boolean) {
        if (monitorBluetooth == bluetoothGranted) return
        monitorBluetooth = null // recorded again only if the start succeeds
        monitor.stop()
        seeding.restart(SystemClock.elapsedRealtime())
        runCatching { monitor.start() }
            .onSuccess { monitorBluetooth = bluetoothGranted }
            .onFailure { Diag.e("presence", "monitor start failed", it) }
        pollSeeding()
    }

    /** Recompute the presence decision from the current signals and apply it: GPS rate, the core's counting flag, and an activity-log line on change. */
    fun evaluatePresence() {
        val t = now()
        if (hud == null) zone = Zone.Unknown
        if (seeding.waiting) { // restart in progress: keep the last decision (the core flag still follows it for a newly opened game)
            engine.setCounting(presence.counting)
            return
        }
        val home = if (seeding.complete) homeDebounce.feed(rawHome(), t) else rawHome()
        val car = if (seeding.complete) carDebounce.feed(rawCar(), t) else rawCar()
        // A debounced change gets no event of its own (GPS may be off), so look again until it has settled.
        main.removeCallbacks(reevaluate)
        if (seeding.complete && (homeDebounce.pending || carDebounce.pending)) main.postDelayed(reevaluate, REEVALUATE_MS)
        val d = PresencePolicy.decide(Signals(playing = hud != null, homeWifi = home, carBluetooth = car, zone = zone))
        // Every run, not only on change: a game that replaces an open one starts counting again. The core ignores an unchanged value.
        engine.setCounting(d.counting)
        if (d == presence) return
        val changedState = d.state != presence.state
        presence = d
        if (changedState) {
            Diag.i("presence", d.state.name, "counting" to d.counting, "gps" to d.gps.toString())
            if (hud != null) engine.logPresence(presenceText(d.state), t)
        }
        applyLocation()
    }

    private fun presenceText(s: PresenceState) = when (s) {
        PresenceState.AtHome -> "Home Wi-Fi connected: paused"
        PresenceState.InCar -> "Car Bluetooth connected: not counting"
        PresenceState.OutsideZones -> "Outside every zone: saving battery"
        PresenceState.InZone -> "Tracking"
        PresenceState.Stopped -> "Stopped playing" // not logged (only with a game open), kept so the when is exhaustive
    }

    var appVisible = true
    /** Start, change or stop location to match the presence decision. */
    fun applyLocation() {
        val rate = if (locationPermitted) GpsPolicy.forDecision(presence, appVisible) else null
        if (rate == null) sensors.stopLocation() else sensors.startLocation(rate)
    }

    /** One line a minute while a game is open: what the sensors delivered, power state and battery. Gaps in these lines are the story. */
    fun heartbeat() {
        val pm = ctx.getSystemService(Context.POWER_SERVICE) as android.os.PowerManager
        val bm = ctx.getSystemService(Context.BATTERY_SERVICE) as android.os.BatteryManager
        Diag.i(
            "heartbeat", "tracking",
            "fixes" to fixesSinceBeat, "rejected" to rejectedSinceBeat,
            "last_fix_age_s" to if (lastFixMs == 0L) -1L else (now() - lastFixMs) / 1000,
            "last_acc_m" to lastFixAcc, "last_provider" to lastFixProvider, "by_provider" to providerCounts.entries.joinToString(",") { "${it.key}=${it.value}" },
            "steps" to stepsTotal, "battery_pct" to bm.getIntProperty(android.os.BatteryManager.BATTERY_PROPERTY_CAPACITY),
            "screen_on" to pm.isInteractive, "power_save" to pm.isPowerSaveMode,
            "doze" to pm.isDeviceIdleMode, "bg_location" to (android.os.Build.VERSION.SDK_INT < 29 || ctx.checkSelfPermission(android.Manifest.permission.ACCESS_BACKGROUND_LOCATION) == android.content.pm.PackageManager.PERMISSION_GRANTED), "unrestricted" to pm.isIgnoringBatteryOptimizations(ctx.packageName),
            "quests" to quests.size, "done" to (hud?.done ?: 0),
            "presence" to presence.state.name, "counting" to presence.counting,
        )
        fixesSinceBeat = 0; rejectedSinceBeat = 0; providerCounts.clear()
        evaluatePresence() // backstop: a settled debounce never waits longer than a minute
        drainCoreDiag()
    }

    /** Move messages the Rust core queued (journal failures etc.) into the diagnostics log. */
    fun drainCoreDiag() = engine.takeDiag().forEach { Diag.w("core", it) }

    // ----------------------------------------------------------------- away report
    /** The app left the screen: the trace has a gap from now on. */
    fun onBackground() {
        engine.saveGame()
        engine.logAppState(false, now())
        Diag.i("lifecycle", "background")
        drainCoreDiag()
    }

    /** Back on screen: if you were gone a while, build the report of what the phone recorded. */
    fun onForeground() {
        val left = engine.lastBackgroundMs()
        val t = now()
        engine.logAppState(true, t)
        Diag.i("lifecycle", "foreground", "away_ms" to (left?.let { t - it } ?: -1L))
        if (left != null && t - left >= AWAY_MIN_MS) away = engine.awayReport(left, t)
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
            .onFailure { fail("save", it); status = "Could not save: ${it.message}" }
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
                .onFailure { fail("scan", it); status = "Couldn't reach the map servers just now. Trying again in a moment." }
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
            .onFailure { fail("save", it); status = "Could not save: ${it.message}" }
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

    /** Saves home. [announce] shows "Home set" in the global status line; screens that show their own confirmation pass false. */
    fun setHome(p: LatLng, announce: Boolean = true) {
        runCatching { engine.setHome(GeoPoint(p.latitude, p.longitude)) }
            .onSuccess { home = engine.home(); if (announce) status = "Home set" }
            .onFailure { fail("set_home", it); status = "Could not set home: ${it.message}" }
    }

    // ------------------------------------------------------------------- games
    fun startSolo(opts: SoloOptionsIn, zoneRealms: List<String>, name: String, awayZoneOnly: Boolean, awayDistanceM: UInt) {
        scope.launch {
            busy = "Building your game..."
            val seed = Random.nextLong().toULong() shr 1
            val r = withContext(Dispatchers.IO) { runCatching { engine.startSolo(UUID.randomUUID().toString(), name, opts, zoneRealms, seed, surfacePref, avoidStairs, awayZoneOnly, awayDistanceM) } }
            busy = null
            r.onSuccess { simClockMs = 0; log.clear(); refreshAll(); tab = 2; status = "Game started!" }
                .onFailure { fail("start_game", it); status = "Could not start: ${it.message}" }
        }
    }

    fun exportYaml(opts: SoloOptionsIn) {
        runCatching { engine.buildYaml("Player", opts) }.onSuccess { yamlText = it }.onFailure { fail("yaml", it); status = "YAML failed: ${it.message}" }
    }

    fun openGame(id: String) {
        runCatching { engine.openGame(id) }.onSuccess { Diag.i("game", "opened", "id" to id); engine.logSession(true, now()); simClockMs = 0; refreshAll(); tab = 2 }.onFailure { fail("open_game", it); status = "Could not open: ${it.message}" }
    }

    /** Stop playing: log it, close the game and go to the Play tab, which then lists the saved games to continue. Tracking stops. */
    fun pause() {
        engine.logSession(false, now())
        Diag.i("game", "paused")
        engine.closeGame()
        simPos = null
        selected = null
        refreshAll()
        refreshActivity()
        tab = 2
    }

    /** Give an unfinished quest a new place (the player's own reroll, not the Shuffle trap). */
    fun reroll(id: Long) {
        runCatching { engine.reroll(listOf(id), (Random.nextLong() ushr 1).toULong()) }.onFailure { fail("reroll", it) }
        refreshPlay()
    }

    fun refreshActivity() { activity = engine.activity(300u) }

    fun deleteGame(id: String) {
        runCatching { engine.deleteGame(id) }
        refreshAll()
    }

    // ------------------------------------------------------------------ events
    fun handle(events: List<EventOut>) {
        events.forEach { Diag.i("event", it.toString().take(300)) }
        for (e in events) when (e) {
            is EventOut.QuestDone -> say("Done: ${e.name}")
            is EventOut.Reward -> say("Found: ${e.item}")
            is EventOut.ZoneUnlocked -> say("Zone ${e.zone} unlocked!")
            is EventOut.Trap -> say(e.message)
            is EventOut.Discovered -> {}
            is EventOut.GoalAchieved -> {
                say("GOAL ACHIEVED: ${e.label}"); status = "You won! ${e.label}"
                if (hud?.backend == "archipelago") runCatching { session?.sendGoal() }.onFailure { fail("ap_goal", it); say("could not report goal: ${it.message}") }
            }
            is EventOut.Info -> say(e.text)
            is EventOut.SendCheck -> runCatching { session?.sendCheck(e.locationId) }.onFailure { fail("ap_check", it); say("check failed: ${it.message}") }
            is EventOut.ShuffleRequested -> runCatching { engine.reroll(emptyList(), Random.nextLong().toULong() shr 1) }
        }
    }

    // Counters for the heartbeat in the diagnostics log (what the phone delivered since the last beat).
    private var fixesSinceBeat = 0
    private var rejectedSinceBeat = 0
    private var lastFixMs = 0L
    private var lastFixAcc = 0f
    private var lastFixProvider = ""
    private val progressBuckets = HashMap<Long, Int>()
    private val providerCounts = HashMap<String, Int>()

    fun onFix(loc: Location) {
        if (!engine.hasGame() || simPos != null) return
        lastFixMs = now(); lastFixAcc = loc.accuracy; lastFixProvider = loc.provider ?: ""
        if (loc.accuracy > 35f) rejectedSinceBeat++ else fixesSinceBeat++
        providerCounts.merge(loc.provider ?: "?", 1, Int::plus)
        handle(engine.onFix(loc.latitude, loc.longitude, now(), loc.accuracy.toDouble(), stepsTotal, false))
        zone = when (engine.zoneProximity(loc.latitude, loc.longitude)) { "inside" -> Zone.Inside; "near" -> Zone.Near; "far" -> Zone.Far; else -> Zone.Unknown }
        evaluatePresence()
        refreshPlay(withTrace = traceThrottle.due(now()))
        logProgress()
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
        return engine.onFix(p.lat, p.lon, tick(advanceMs), 5.0, if (steps) simSteps else null, true)
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
                "roundtrip" -> { a?.let { out += fix(it, 600_000) }; out += fix(home, 600_000) }
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
        s.status().let { if (it != apStatus) Diag.i("ap", "status", "status" to it); apStatus = it }
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

    fun startApGame(zoneRealms: List<String>, name: String, awayZoneOnly: Boolean, awayDistanceM: UInt) {
        val json = apSlotJson ?: run { status = "Connect first"; return }
        scope.launch {
            val seed = Random.nextLong().toULong() shr 1
            val r = withContext(Dispatchers.IO) { runCatching { engine.startArchipelago(UUID.randomUUID().toString(), name, json, "archipelago", zoneRealms, seed, surfacePref, avoidStairs, awayZoneOnly, awayDistanceM) } }
            r.onSuccess { simClockMs = 0; apSyncedChecked = false; refreshAll(); tab = 2; status = "Archipelago game started" }
                .onFailure { fail("start_game", it); status = "Could not start: ${it.message}" }
        }
    }

    /** The goals of the connected game in words ("Letter Hunt, The Big One (any one)"), from slot_data v3 (or the single goal of v2). */
    fun apGoalSummary(): String? = apSlotJson?.let { json ->
        runCatching {
            val o = JSONObject(json)
            val goals = o.optJSONArray("goals")
            if (goals == null) {
                o.optString("goal").ifBlank { null }
            } else {
                val names = (0 until goals.length()).map { goals.getJSONObject(it).getString("id").replace('_', ' ') }
                val rule = when (o.optString("goal_requirement")) { "all" -> "all" ; "at_least" -> "at least ${o.optInt("goal_need")}" ; else -> "any one" }
                if (names.size == 1) names[0] else "${names.joinToString(", ")} ($rule)"
            }
        }.getOrNull()
    }
}

/** At most one download per realm in this time. */
private const val SCAN_COOLDOWN_MS = 20_000L

/** A scan needing this many downloads asks first (about 20 tiles). */
private const val BIG_SCAN_REQUESTS = 60

private const val QUIET_RETRY_MS = 45_000L
private const val MAX_QUIET_RETRIES = 4
