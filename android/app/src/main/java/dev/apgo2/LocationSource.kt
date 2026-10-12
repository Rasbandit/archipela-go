package dev.apgo2

import android.annotation.SuppressLint
import android.content.Context
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.location.LocationRequest
import android.os.Build
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.core.content.ContextCompat
import com.google.android.gms.location.LocationCallback
import com.google.android.gms.location.LocationResult
import com.google.android.gms.location.LocationServices
import com.google.android.gms.tasks.CancellationTokenSource

private const val TAG = SENSORS_TAG
private const val FLUSH_REQUEST = 1
private const val FLUSH_TIMEOUT_MS = 2_000L

/**
 * Fixes: Play services' fused location or a LocationManager provider ([GpsPolicy.providers]), the once-per-session cold-start fix,
 * and the fall-back to LocationManager when Play services refuses. Part of [Sensors]; callers hold the location permission.
 */
internal class LocationSource(
    private val ctx: Context,
    private val model: AppModel,
    gms: Lazy<Boolean>,
    private val gnssEvidence: GnssEvidence,
) {
    private val lm = ctx.getSystemService(Context.LOCATION_SERVICE) as LocationManager
    private var locationListener: LocationListener? = null
    private var rate: GpsPolicy.Rate? = null

    // The cold start is asked once per session: again only after location fully stopped or a game opened ([allowColdStart]).
    private var coldStartDone = false
    private var coldStartCancel: CancellationSignal? = null
    private var gmsColdStartCancel: CancellationTokenSource? = null

    // Play services' fused location (one client for the session) and its callback while registered.
    private val fusedLocation by lazy { LocationServices.getFusedLocationProviderClient(ctx) }
    private var gmsCallback: LocationCallback? = null

    // Play services refused location updates: LocationManager until the next game opens ([allowColdStart]).
    private var gmsLocationFailed = false

    // Detected by package and by Play services saying it is usable ([Sensors]).
    private val gms: Boolean by gms

    // Context.getMainExecutor is Android 9+; this works from 8.
    private val mainExecutor by lazy { ContextCompat.getMainExecutor(ctx) }

    private val locationRunning get() = locationListener != null || gmsCallback != null

    // The LocationManager provider [locationListener] listens to (a flush names it).
    private var managerProvider: String? = null

    // A flush in flight (its token), the rate to start when it completes, and the LocationManager completion to call.
    private var flushing: Any? = null
    private var afterFlush: GpsPolicy.Rate? = null
    private var onFlushed: (() -> Unit)? = null
    private val flushTimeout = Handler(Looper.getMainLooper())

    private fun plan(): GpsPolicy.ProviderPlan {
        val enabled = lm.allProviders.filter { lm.isProviderEnabled(it) }.toSet()
        return GpsPolicy.providers(enabled, gms && !gmsLocationFailed)
    }

    /**
     * Start (or re-start with a new [rate]). Calling again with the same rate does nothing. Leaving a batched request flushes it
     * first and swaps the request when the flush completed ([GpsPolicy.shouldFlush]); a later call meanwhile only replaces the rate
     * the swap will use.
     */
    fun startLocation(rate: GpsPolicy.Rate) {
        when {
            flushing != null -> afterFlush = rate
            locationRunning && this.rate == rate -> Unit
            locationRunning && GpsPolicy.shouldFlush(this.rate, rate) && flushThen(rate) -> Unit
            else -> register(rate)
        }
    }

    // Ask the running request to deliver its batch now; [next] is started once that is done (or after FLUSH_TIMEOUT_MS, so a flush
    // that never reports back cannot hold the rate). False when this request cannot flush (LocationManager before Android 12).
    private fun flushThen(next: GpsPolicy.Rate): Boolean {
        val token = Any()
        val done = {
            if (flushing === token) {
                flushing = null
                onFlushed = null
                flushTimeout.removeCallbacksAndMessages(null)
                val wanted = afterFlush
                afterFlush = null
                // The latest rate asked for, registered directly: startLocation would see the batch still running and flush again.
                wanted?.takeIf { it != rate }?.let(::register)
            }
        }
        val l = locationListener
        val p = managerProvider
        val started =
            runCatching {
                when {
                    gmsCallback != null -> {
                        fusedLocation.flushLocations().addOnCompleteListener(mainExecutor) { done() }
                        true
                    }

                    l != null && p != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
                        onFlushed = done
                        lm.requestFlush(p, l, FLUSH_REQUEST)
                        true
                    }

                    else -> {
                        false
                    }
                }
            }.onFailure { Diag.warn(TAG, "location flush failed", "error" to it.toString()) }.getOrDefault(false)
        if (!started) return false
        flushing = token
        afterFlush = next
        flushTimeout.postDelayed({ done() }, FLUSH_TIMEOUT_MS)
        Diag.info(TAG, "location flush", "next_interval_ms" to next.intervalMs)
        return true
    }

    private fun register(rate: GpsPolicy.Rate) {
        unregister()
        // One provider only: mixing them interleaved 100 m-off network fixes with good GPS fixes and made the position jump streets.
        val plan = plan()
        val main = plan.main
        val registered =
            when (main) {
                null -> false
                GpsPolicy.GMS_FUSED -> startGmsUpdates(rate)
                else -> startManagerUpdates(main, rate)
            }
        // Nothing registered (no permission yet): remember nothing, so a later call with the same rate tries again.
        if (!registered) return
        if (!coldStartDone) plan.coldStart?.let(::coldStartOnce)
        this.rate = rate
        // Screen-off fixes come far apart: the core must not take the space between them for lost GPS.
        model.engine.setFixSpacingHint(rate.fixSpacingMs)
        Diag.info(
            TAG,
            "location started",
            "interval_ms" to rate.intervalMs,
            "min_dist_m" to rate.minDistanceM,
            "high_accuracy" to rate.highAccuracy,
            "max_delay_ms" to rate.maxDelayMs,
            "gms" to gms,
            "fused_listed" to (FUSED_SOURCE in lm.allProviders),
            "providers" to listOfNotNull(plan.main, plan.coldStart).joinToString(","),
        )
    }

    private fun coldStartOnce(provider: String) {
        runCatching { if (provider == GpsPolicy.GMS_FUSED) gmsColdStart() else coldStart(provider) }
            .onSuccess { coldStartDone = true }
            .onFailure { Diag.error(TAG, "cold start fix failed for $provider", it) }
    }

    @SuppressLint("MissingPermission")
    private fun startManagerUpdates(
        provider: String,
        rate: GpsPolicy.Rate,
    ): Boolean {
        val l =
            object : LocationListener {
                override fun onLocationChanged(loc: Location) = deliver(loc)

                // Android 12+ hands a screen-off batch over at once; the filter needs them in the order they were taken.
                override fun onLocationChanged(locations: List<Location>) =
                    FixTime.oldestFirst(locations) { it.elapsedRealtimeNanos }.forEach(::deliver)

                // Android 12+: the batch asked for by [flushThen] was delivered.
                override fun onFlushComplete(requestCode: Int) {
                    if (requestCode == FLUSH_REQUEST) onFlushed?.invoke()
                }
            }
        return runCatching {
            request(provider, rate, l)
            locationListener = l
            managerProvider = provider
            lm.getLastKnownLocation(provider)?.let { model.realLoc = it }
        }.onFailure {
            Diag.error(TAG, "requestLocationUpdates failed for $provider", it)
            lm.removeUpdates(l)
        }.isSuccess
    }

    // Google's fused location: batches arrive as one result (oldest first, sorted again to be sure). If Play services refuses later
    // (the task fails), this session falls back to LocationManager.
    @SuppressLint("MissingPermission")
    private fun startGmsUpdates(rate: GpsPolicy.Rate): Boolean {
        val cb =
            object : LocationCallback() {
                override fun onLocationResult(result: LocationResult) =
                    FixTime.oldestFirst(result.locations) { it.elapsedRealtimeNanos }.forEach(::deliverGms)
            }
        // Set first: the failure check below compares against it.
        gmsCallback = cb
        return runCatching {
            fusedLocation
                .requestLocationUpdates(GpsPolicy.gmsRequest(rate), mainExecutor, cb)
                .addOnFailureListener(mainExecutor) { e ->
                    // Still the current request (not stopped or replaced meanwhile): use LocationManager instead.
                    if (gmsCallback === cb) {
                        Diag.error(TAG, "requestLocationUpdates failed for ${GpsPolicy.GMS_FUSED}", e)
                        gmsLocationFailed = true
                        // The Play services cold start may never come: let the network one run once.
                        coldStartDone = false
                        // The latest rate asked for (a flush of this failed request waits for it), not the one that failed.
                        val wanted = afterFlush ?: rate
                        cancelFlush()
                        register(wanted)
                    }
                }
            fusedLocation.lastLocation.addOnSuccessListener(mainExecutor) { loc ->
                // Only as the marker's start, and never over a newer fix that came first.
                if (loc != null && (model.realLoc?.elapsedRealtimeNanos ?: 0L) < loc.elapsedRealtimeNanos) model.realLoc = loc
            }
        }.onFailure {
            Diag.error(TAG, "requestLocationUpdates failed for ${GpsPolicy.GMS_FUSED}", it)
            gmsCallback = null
        }.isSuccess
    }

    // Play services names its fixes "fused" (the core's Fused provider); set it so a blank or odd name never reads as Other. A fused
    // fix while the GNSS chip uses no satellite is a Wi-Fi or cell position: "network", display only (adversarial review I2).
    private fun deliverGms(loc: Location) {
        val gpsOn = runCatching { lm.isProviderEnabled(LocationManager.GPS_PROVIDER) }.getOrDefault(true)
        loc.provider = gnssEvidence.providerOf(FUSED_SOURCE, SystemClock.elapsedRealtime(), gpsOn)
        deliver(loc)
    }

    private fun deliver(loc: Location) {
        val sample = FixSamples.from(loc, System.currentTimeMillis(), SystemClock.elapsedRealtimeNanos())
        Diag.raw("rawfix", RawLines.fix(sample))
        model.realLoc = loc
        model.onFix(loc, sample)
    }

    // Android 12+ takes an explicit quality and a batching delay; older Android only the interval and distance. The gps provider is
    // the chip either way.
    @SuppressLint("MissingPermission")
    private fun request(
        provider: String,
        rate: GpsPolicy.Rate,
        l: LocationListener,
    ) {
        val spec = GpsPolicy.request(rate)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
            return lm.requestLocationUpdates(provider, spec.intervalMs, spec.minDistanceM, l)
        }
        val req =
            LocationRequest
                .Builder(spec.intervalMs)
                .setMinUpdateIntervalMillis(spec.minIntervalMs)
                .setMaxUpdateDelayMillis(spec.maxDelayMs)
                .setMinUpdateDistanceMeters(spec.minDistanceM)
                .setQuality(spec.quality)
                .build()
        lm.requestLocationUpdates(provider, req, ctx.mainExecutor, l)
    }

    // One network fix to start from while the GNSS chip searches (no Play services); the core inflates its error 4x and drops network fixes
    // once GNSS fixes arrive. Before Android 11 only a last-known fix is there, used if at most 2 minutes old.
    @SuppressLint("MissingPermission")
    private fun coldStart(provider: String) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val cancel = CancellationSignal().also { coldStartCancel = it }
            lm.getCurrentLocation(provider, cancel, ctx.mainExecutor) { loc -> loc?.let(::deliver) }
        } else {
            lm
                .getLastKnownLocation(provider)
                ?.takeIf { FixTime.fresh(it.elapsedRealtimeNanos, SystemClock.elapsedRealtimeNanos()) }
                ?.let(::deliver)
        }
    }

    // Play services' one fix to start from (cancelled with location, like the LocationManager one).
    @SuppressLint("MissingPermission")
    private fun gmsColdStart() {
        val cancel = CancellationTokenSource().also { gmsColdStartCancel = it }
        fusedLocation
            .getCurrentLocation(GpsPolicy.gmsColdStart(), cancel.token)
            .addOnSuccessListener(mainExecutor) { loc -> loc?.let(::deliverGms) }
            .addOnFailureListener(mainExecutor) { e ->
                Diag.warn(
                    TAG,
                    "cold start fix failed for ${GpsPolicy.GMS_FUSED}",
                    "error" to e.toString(),
                )
            }
    }

    /** A new game was opened: its first location start may ask for a cold-start fix again, and Play services is tried again. */
    fun allowColdStart() {
        coldStartDone = false
        gmsLocationFailed = false
    }

    /** Stop location: listener, any pending cold-start fix, and the once-per-session cold-start mark. */
    fun stopLocation() {
        cancelFlush()
        unregister()
        coldStartCancel?.cancel()
        coldStartCancel = null
        gmsColdStartCancel?.cancel()
        gmsColdStartCancel = null
        coldStartDone = false
    }

    // A flush in flight no longer starts its rate.
    private fun cancelFlush() {
        flushing = null
        afterFlush = null
        onFlushed = null
        flushTimeout.removeCallbacksAndMessages(null)
    }

    // Remove the listener only (a rate change re-registers; a pending cold-start fix stays wanted).
    private fun unregister() {
        if (locationRunning) Diag.info(TAG, "location stopped")
        locationListener?.let { lm.removeUpdates(it) }
        gmsCallback?.let { fusedLocation.removeLocationUpdates(it) }
        locationListener = null
        managerProvider = null
        gmsCallback = null
        rate = null
    }
}
