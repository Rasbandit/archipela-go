package dev.apgo2

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.presence.Debouncer
import dev.apgo2.presence.Decision
import dev.apgo2.presence.GeoFix
import dev.apgo2.presence.GpsMode
import dev.apgo2.presence.HomeNetwork
import dev.apgo2.presence.HomeWifiOffer
import dev.apgo2.presence.OfferSignals
import dev.apgo2.presence.PresenceMonitor
import dev.apgo2.presence.PresencePolicy
import dev.apgo2.presence.PresenceSeeding
import dev.apgo2.presence.PresenceSignals
import dev.apgo2.presence.PresenceState
import dev.apgo2.presence.Signals
import dev.apgo2.presence.WifiId
import dev.apgo2.presence.Zone
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.apgo_ffi.CircleOut
import uniffi.apgo_ffi.PositionOut
import uniffi.apgo_ffi.PresenceHome

private const val SETTLE_MARGIN_MS = 50L // a look scheduled for the exact due time must not land a hair early
private const val SEED_TIMEOUT_MS = 3_000L
private const val TAG = "presence"
private const val NANOS_PER_MS = 1_000_000L

/**
 * Decides, from the Wi-Fi, Bluetooth and zone signals, whether the player is playing, and applies it: the GPS rate, the
 * core's counting flag and an activity-log line on change.
 */
internal class PresenceController(
    private val model: AppModel,
    ctx: Context,
    private val scope: CoroutineScope,
) {
    val monitor = PresenceMonitor(ctx) { pollSeeding() }

    /** What the presence rules decided last (the status chip shows its state). */
    var decision by mutableStateOf(Decision(PresenceState.Stopped, GpsMode.Off, counting = false))
        private set
    var locationPermitted by mutableStateOf(false)

    /** The network the "You're home: add this Wi-Fi?" dialog offers, or `null` (see [HomeWifiOffer]). */
    var homeOffer by mutableStateOf<WifiId?>(null)
        private set
    private var offerDismissedAtMs: Long? = null
    var appVisible = true
        set(v) {
            if (field != v) Diag.raw("rawstate", RawLines.state(decision.state.name, decision.counting, zone.name.lowercase(), v))
            field = v
        }

    /** Whether the screen is on (the in-zone rate is 1 s with it on and the app in front, else 5 s batched: [GpsPolicy]). */
    var screenOn = true
        private set
    private val homeDebounce = Debouncer()
    private val carDebounce = PresencePolicy.carDebouncer()
    private var zone = Zone.Unknown

    // The last raw car reading logged: the diag shows when Bluetooth really dropped, next to when the state followed.
    private var loggedCar: Boolean? = null
    private val main = Handler(Looper.getMainLooper())
    private val reevaluate = Runnable { evaluate() }
    private val seedPoll = Runnable { pollSeeding() }
    private val seeding = PresenceSeeding(SEED_TIMEOUT_MS)
    private var monitorBluetooth: Boolean? = null

    // The core's home answer as of the last evaluation, and whether it was started for this monitor run.
    private var homeState = PresenceHome.AWAY
    private var homeStarted = false

    /** The home circle presence uses (the map draws it), or `null` while home Wi-Fi alone decides. Set on every evaluation. */
    var homeZone by mutableStateOf<CircleOut?>(null)
        private set

    /**
     * Start the Wi-Fi/Bluetooth watcher once location permission is there, and again only when the Bluetooth grant changes.
     * It is never stopped within the process life: it belongs to the model (like the sensors), not to the activity, so rotation or
     * swiping the app away cannot leave the presence state stale while the tracking service keeps the game running. It is cheap
     * (two registered callbacks). A failed start is not recorded, so the next call retries.
     */
    fun ensureMonitor(bluetoothGranted: Boolean) {
        if (monitorBluetooth == bluetoothGranted) return
        monitorBluetooth = null // recorded again only if the start succeeds
        homeStarted = false // the core decides home afresh from the new readings
        monitor.stop()
        seeding.restart(SystemClock.elapsedRealtime())
        runCatching { monitor.start() }
            .onSuccess { monitorBluetooth = bluetoothGranted }
            .onFailure { Diag.error(TAG, "monitor start failed", it) }
        pollSeeding()
    }

    /**
     * Start presence with no screen (the tracking service was restarted by Android): take the grants the activity would pass in,
     * start the watcher and the GPS rate the decision calls for. The activity takes over again when it comes up.
     */
    fun startHeadless(
        locationGranted: Boolean,
        bluetoothGranted: Boolean,
    ) {
        appVisible = false
        locationPermitted = locationGranted
        if (locationGranted) ensureMonitor(bluetoothGranted) else applyLocation()
    }

    /** Where the last fix put the player relative to the game's zones ("inside", "near", "far"; anything else is unknown). */
    fun updateZone(proximity: String) {
        zone =
            when (proximity) {
                "inside" -> Zone.Inside
                "near" -> Zone.Near
                "far" -> Zone.Far
                else -> Zone.Unknown
            }
    }

    /**
     * Recompute the presence decision from the current signals and apply it: GPS rate, the core's counting flag, and an
     * activity-log line on change.
     */
    fun evaluate() {
        val t = model.now()
        if (model.hud == null) zone = Zone.Unknown
        if (seeding.waiting) { // restart in progress: keep the last decision (the core flag still follows it for a newly opened game)
            model.engine.setCounting(decision.counting, t)
            applyLocation() // holds GPS off for an open game until home is known
            return
        }
        checkHomeOffer(t)
        val home = if (seeding.complete) homeDebounce.feed(rawHome(), t) else rawHome()
        val car = if (seeding.complete) carDebounce.feed(rawCar(), t) else rawCar()
        homeState = coreHome(home, t)
        // A debounced change or the end of a home check gets no event of its own (GPS may be off): look once more exactly then.
        main.removeCallbacks(reevaluate)
        val checkEnds = model.engine.presenceDueMs()?.let { (it - t).coerceAtLeast(0) }
        val settle = listOfNotNull(homeDebounce.settlesInMs(t), carDebounce.settlesInMs(t), checkEnds).minOrNull()
        if (seeding.complete && settle != null) main.postDelayed(reevaluate, settle + SETTLE_MARGIN_MS)
        val signals =
            Signals(
                playing = model.hud != null,
                home = homeState != PresenceHome.AWAY,
                carBluetooth = car,
                zone = zone,
                checking = homeState == PresenceHome.CHECKING,
            )
        val d = PresencePolicy.decide(signals)
        // Every run, not only on change: a game that replaces an open one starts counting again. The core ignores an unchanged value.
        model.engine.setCounting(d.counting, t)
        model.due.schedule() // leaving or reaching home moves the next due time
        if (d == decision) {
            applyLocation() // unchanged decision, but the hold above may just have ended (cheap: a same rate is a no-op)
            return
        }
        val prev = decision
        decision = d
        Diag.raw("rawstate", RawLines.state(d.state.name, d.counting, zone.name.lowercase(), appVisible))
        if (d.state != prev.state) onStateChange(prev.state, d, t)
        // Also a home check that ends at home: the state stays AtHome, only its GPS mode changes.
        val arrived = PresencePolicy.arrivedHome(prev.state, d.state, d.gps == GpsMode.Check, prev.gps == GpsMode.Check)
        if (arrived && model.hud != null) bankAtHome()
        applyLocation()
    }

    // The core decides home from home Wi-Fi (debounced here) and the circle round the home pin; it learns of fixes and steps itself.
    // It acts only on a changed Wi-Fi reading, so passing it every time is safe.
    private fun coreHome(
        wifi: Boolean?,
        t: Long,
    ): PresenceHome {
        val e = model.engine
        e.presenceSetWalkSensor(model.sensors.stepsAvailable)
        homeZone = e.presenceCircles().firstOrNull()
        e.presenceWifiCamp(
            if (wifi == true) {
                "home"
            } else if (wifi == false) {
                "other"
            } else {
                null
            },
            t,
        )
        if (!homeStarted) {
            e.presenceStart(t)
            homeStarted = true
        }
        e.presenceTick(t)
        return e.presenceState()
    }

    /** A step batch reached the core: a walk at home may have started its GPS check, so decide again only if its answer moved. */
    fun onSteps() {
        if (homeStarted && model.engine.presenceState() != homeState) evaluate()
    }

    // The presence state changed: log it, and let a newly opened game ask for a cold-start fix.
    private fun onStateChange(
        prev: PresenceState,
        d: Decision,
        t: Long,
    ) {
        Diag.info(TAG, d.state.name, "counting" to d.counting, "gps" to d.gps.toString())
        if (model.hud != null) model.engine.logPresence(presenceText(d.state), t)
        model.noteJournal()
        if (prev == PresenceState.Stopped) model.sensors.allowColdStart()
    }

    // Presence arrived home (home Wi-Fi or the home circle): forager quests bank what they carry, even with no GPS fix. The core
    // reads and thins the walk's trace for the recap, so it runs off the main thread; the events and the redraw come back to it.
    private fun bankAtHome() {
        if (!model.engine.hasGame()) return
        val t = model.now()
        scope.launch {
            val r = withContext(Dispatchers.IO) { runCatching { model.engine.bankAtCamp(t) } }
            r.onFailure { Diag.error(TAG, "bank at home failed", it) }
            model.handle(r.getOrDefault(emptyList())) // a new recap comes back as RecapReady
            model.refreshPlay(withTrace = false)
        }
    }

    /** Follow screen on/off for the life of the process: each change re-applies the location rate (an unchanged rate is a no-op). */
    fun watchScreen(ctx: Context) {
        val r =
            object : android.content.BroadcastReceiver() {
                override fun onReceive(
                    c: Context,
                    i: android.content.Intent,
                ) {
                    screenOn = i.action == android.content.Intent.ACTION_SCREEN_ON
                    applyLocation()
                }
            }
        val filter =
            android.content.IntentFilter().apply {
                addAction(android.content.Intent.ACTION_SCREEN_ON)
                addAction(android.content.Intent.ACTION_SCREEN_OFF)
            }
        androidx.core.content.ContextCompat
            .registerReceiver(ctx, r, filter, androidx.core.content.ContextCompat.RECEIVER_NOT_EXPORTED)
        // Read after registering, so a change in between is not lost.
        screenOn = ctx.getSystemService(android.os.PowerManager::class.java).isInteractive
    }

    /** Start, change or stop location to match the presence decision. */
    fun applyLocation() {
        val holding = (seeding.waiting || seeding.unstarted) && model.hud != null
        val rate = if (locationPermitted) GpsPolicy.forDecision(decision, appVisible, screenOn, holding) else null
        if (rate == null) model.sensors.stopLocation() else model.sensors.startLocation(rate)
        val inZone = rate != null && decision.state == PresenceState.InZone
        // Satellite status wakes the main thread every second: only while the map is in front (it would undo screen-off batching).
        if (inZone && screenOn && appVisible) model.sensors.startGnss() else model.sensors.stopGnss()
        if (inZone) {
            model.sensors.startHeading(
                CompassRate.periodUs(screenOn, appVisible),
                onScreen = screenOn && appVisible,
            )
        } else {
            model.sensors.stopHeading()
        }
        // Home or car signal gone but not yet confirmed: count steps from the door (backfilled once it is).
        val leaving = StepRate.leaving(decision.state, homeDebounce.pending, carDebounce.pending)
        val onScreen = screenOn && appVisible
        val homeCircle = homeZone != null
        model.sensors.setStepBatch(
            StepRate.batchUs(decision.state, inZone, leaving, onScreen, homeCircle),
            StepRate.wakeUp(decision.state, inZone, leaving, onScreen, homeCircle),
        )
    }

    // Runs after every fix and Wi-Fi change (both end in [evaluate]). A simulated position is not a real visit home. The fix is the
    // filter's estimate while it is from GPS ([HomeOfferFix]); otherwise the raw fix.
    private fun checkHomeOffer(t: Long) {
        val pos = if (model.simPos == null) model.engine.position(t) else null
        val loc = model.realLoc?.takeIf { it.hasAccuracy() && model.simPos == null }
        val (fix, fixAgeMs) =
            HomeOfferFix.choose(
                pos,
                loc?.let { GeoFix(it.latitude, it.longitude, it.accuracy.toDouble()) },
                // Monotonic age, immune to GPS/wall-clock skew; the "Later" cooldown stays on the wall clock (nowMs).
                loc?.let { (SystemClock.elapsedRealtimeNanos() - it.elapsedRealtimeNanos) / NANOS_PER_MS },
            )
        val pin = model.realmOps.homePoint()
        val signals =
            OfferSignals(
                saved = model.settings.homeNetworks,
                playing = model.hud != null,
                fix = fix,
                fixAgeMs = fixAgeMs,
                home = pin?.let { GeoFix(it.lat, it.lon, 0.0) },
                wifi = monitor.currentWifi,
                muted = model.settings.mutedHomeOffers,
                showing = homeOffer != null,
                dismissedAtMs = offerDismissedAtMs,
                nowMs = t,
            )
        val next = HomeWifiOffer.next(homeOffer, signals)
        if (next != homeOffer) Diag.info(TAG, if (next == null) "home wifi offer withdrawn" else "home wifi offer")
        homeOffer = next
    }

    /** "Add": save the offered network as home; the at-home rule takes over from here. */
    fun acceptHomeOffer() {
        checkHomeOffer(model.now()) // things may have changed while the dialog was up (e.g. Wi-Fi saved in setup)
        val w = homeOffer ?: return
        homeOffer = null
        model.settings.addHome(HomeNetwork(w.ssid ?: return, w.bssid))
        evaluate()
    }

    /** "Not this one": never offer this network again. */
    fun muteHomeOffer() {
        homeOffer?.ssid?.let { model.settings.muteHomeOffer(it) }
        homeOffer = null
    }

    /** "Later": ask again in a while (in memory only). */
    fun dismissHomeOffer() {
        homeOffer = null
        offerDismissedAtMs = model.now()
    }

    private fun rawHome() = PresenceSignals.isHome(monitor.currentWifi, model.settings.homeNetworks)

    private fun rawCar() =
        PresenceSignals.carConnected(monitor.connectedCarCandidates, model.settings.carDevices).also {
            if (it != loggedCar) Diag.info("presence", "car bluetooth", "connected" to it.toString())
            loggedCar = it
        }

    // Each debouncer takes its first real reading once per monitor start, on its own signal (see [PresenceSeeding]); a signal that
    // never answers (not on Wi-Fi, Bluetooth off) is read as it stands after the timeout. This one runnable reschedules itself while
    // seeding is incomplete and is never cancelled by monitor events, which only call this too.
    // Known limit: after a process start the first game open waits for seeding (up to 3 s) with the previous decision (Stopped:
    // not counting, idle GPS rate).
    private fun pollSeeding() {
        val plan = seeding.poll(SystemClock.elapsedRealtime(), monitor.wifiReported, monitor.bluetoothReady)
        if (plan.home) homeDebounce.seed(rawHome())
        if (plan.car) carDebounce.seed(rawCar())
        // The monitor's callbacks seed early when a reading arrives; otherwise look once, at the timeout.
        main.removeCallbacks(seedPoll)
        seeding.timeoutInMs(SystemClock.elapsedRealtime())?.let { main.postDelayed(seedPoll, it + SETTLE_MARGIN_MS) }
        evaluate()
    }

    private fun presenceText(s: PresenceState) =
        when (s) {
            PresenceState.AtHome -> "At home: paused"
            PresenceState.InCar -> "Car Bluetooth connected: not counting"
            PresenceState.OutsideZones -> "Outside every zone: saving battery"
            PresenceState.InZone -> "Tracking"
            PresenceState.Stopped -> "Stopped playing" // not logged (only with a game open), kept so the when is exhaustive
        }
}

/** The fix the home Wi-Fi offer judges, with its age: the estimate while it is from GPS (not bridged, predicted, stale), else raw. */
internal object HomeOfferFix {
    fun choose(
        est: PositionOut?,
        raw: GeoFix?,
        rawAgeMs: Long?,
    ): Pair<GeoFix?, Long?> = if (est?.source == "gps") GeoFix(est.estLat, est.estLon, est.uncertaintyM) to est.ageMs else raw to rawAgeMs
}
