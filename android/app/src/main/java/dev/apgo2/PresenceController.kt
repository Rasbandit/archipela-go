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

private const val REEVALUATE_MS = 5_000L
private const val SEED_TIMEOUT_MS = 3_000L
private const val SEED_POLL_MS = 1_000L
private const val TAG = "presence"

/**
 * Decides, from the Wi-Fi, Bluetooth and zone signals, whether the player is playing, and applies it: the GPS rate, the
 * core's counting flag and an activity-log line on change.
 */
internal class PresenceController(
    private val model: AppModel,
    ctx: Context,
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
    private val homeDebounce = Debouncer()
    private val carDebounce = Debouncer()
    private var zone = Zone.Unknown
    private val main = Handler(Looper.getMainLooper())
    private val reevaluate = Runnable { evaluate() }
    private val seedPoll = Runnable { pollSeeding() }
    private val seeding = PresenceSeeding(SEED_TIMEOUT_MS)
    private var monitorBluetooth: Boolean? = null

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
            .onFailure { Diag.error(TAG, "monitor start failed", it) }
        pollSeeding()
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
            model.engine.setCounting(decision.counting)
            return
        }
        checkHomeOffer(t)
        val home = if (seeding.complete) homeDebounce.feed(rawHome(), t) else rawHome()
        val car = if (seeding.complete) carDebounce.feed(rawCar(), t) else rawCar()
        // A debounced change gets no event of its own (GPS may be off), so look again until it has settled.
        main.removeCallbacks(reevaluate)
        if (seeding.complete && (homeDebounce.pending || carDebounce.pending)) main.postDelayed(reevaluate, REEVALUATE_MS)
        val d = PresencePolicy.decide(Signals(playing = model.hud != null, homeWifi = home, carBluetooth = car, zone = zone))
        // Every run, not only on change: a game that replaces an open one starts counting again. The core ignores an unchanged value.
        model.engine.setCounting(d.counting)
        if (d == decision) return
        val changedState = d.state != decision.state
        decision = d
        if (changedState) {
            Diag.info(TAG, d.state.name, "counting" to d.counting, "gps" to d.gps.toString())
            if (model.hud != null) model.engine.logPresence(presenceText(d.state), t)
        }
        applyLocation()
    }

    /** Start, change or stop location to match the presence decision. */
    fun applyLocation() {
        val rate = if (locationPermitted) GpsPolicy.forDecision(decision, appVisible) else null
        if (rate == null) model.sensors.stopLocation() else model.sensors.startLocation(rate)
    }

    // Runs after every fix and Wi-Fi change (both end in [evaluate]). A simulated position is not a real visit home.
    private fun checkHomeOffer(t: Long) {
        val loc = model.realLoc?.takeIf { it.hasAccuracy() && model.simPos == null }
        val pin = model.realmOps.homePoint()
        val offer =
            HomeWifiOffer.decide(
                OfferSignals(
                    saved = model.settings.homeNetworks,
                    playing = model.hud != null,
                    fix = loc?.let { GeoFix(it.latitude, it.longitude, it.accuracy.toDouble()) },
                    home = pin?.let { GeoFix(it.lat, it.lon, 0.0) },
                    wifi = monitor.currentWifi,
                    muted = model.settings.mutedHomeOffers,
                    showing = homeOffer != null,
                    dismissedAtMs = offerDismissedAtMs,
                    nowMs = t,
                ),
            )
        if (offer != null) {
            Diag.info(TAG, "home wifi offer")
            homeOffer = offer
        }
    }

    /** "Add": save the offered network as home; the at-home rule takes over from here. */
    fun acceptHomeOffer() {
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

    private fun rawCar() = PresenceSignals.carConnected(monitor.connectedCarCandidates, model.settings.carDevices)

    // Each debouncer takes its first real reading once per monitor start, on its own signal (see [PresenceSeeding]); a signal that
    // never answers (not on Wi-Fi, Bluetooth off) is read as it stands after the timeout. This one runnable reschedules itself while
    // seeding is incomplete and is never cancelled by monitor events, which only call this too.
    // Known limit: after a process start the first game open waits for seeding (up to 3 s) with the previous decision (Stopped:
    // not counting, idle GPS rate).
    private fun pollSeeding() {
        val plan = seeding.poll(SystemClock.elapsedRealtime(), monitor.wifiReported, monitor.bluetoothReady)
        if (plan.home) homeDebounce.seed(rawHome())
        if (plan.car) carDebounce.seed(rawCar())
        main.removeCallbacks(seedPoll)
        if (seeding.waiting) main.postDelayed(seedPoll, SEED_POLL_MS)
        evaluate()
    }

    private fun presenceText(s: PresenceState) =
        when (s) {
            PresenceState.AtHome -> "Home Wi-Fi connected: paused"
            PresenceState.InCar -> "Car Bluetooth connected: not counting"
            PresenceState.OutsideZones -> "Outside every zone: saving battery"
            PresenceState.InZone -> "Tracking"
            PresenceState.Stopped -> "Stopped playing" // not logged (only with a game open), kept so the when is exhaustive
        }
}
