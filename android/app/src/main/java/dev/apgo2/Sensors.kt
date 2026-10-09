package dev.apgo2

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.location.GnssStatus
import android.location.LocationManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability

// The log tag of the sensor sources.
internal const val SENSORS_TAG = "sensors"

// Play services' fused provider: the name of its fixes, and of its orientation as a heading source.
internal const val FUSED_SOURCE = "fused"

// How long the step counter may hold readings back before delivering them (microseconds).
private const val STEP_BATCH_US = 10_000_000
private const val FAST_STEP_BATCH_US = 2_000_000
private const val GNSS_LINE_MS = 10_000L
private const val TAG = SENSORS_TAG
private const val GMS_PACKAGE = "com.google.android.gms"

/**
 * Location and step-counter listeners. They belong to the app model, not to the activity, so they keep running
 * (with [TrackingService] holding the process in the foreground) when the screen is off or the activity is gone.
 * Callers must hold the matching permissions before calling the start functions.
 */
internal class Sensors(
    private val ctx: Context,
    private val model: AppModel,
) {
    private val lm = ctx.getSystemService(Context.LOCATION_SERVICE) as LocationManager
    private val sm = ctx.getSystemService(Context.SENSOR_SERVICE) as SensorManager
    private var stepListener: SensorEventListener? = null
    private var fastSteps = false
    private var gnssCallback: GnssStatus.Callback? = null
    private var gnssHardwareLogged = false
    private var gnssFailLogged = false
    private val gnssThrottle = Throttle(GNSS_LINE_MS)

    // Whether the GNSS chip uses satellites now: fused fixes without them reach the core as network (adversarial review I2).
    private val gnssEvidence = GnssEvidence()

    // Detected by package and by Play services saying it is usable (the Play services library is only used when this is true).
    private val gms =
        lazy {
            appEnabled(GMS_PACKAGE) &&
                runCatching { GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(ctx) == ConnectionResult.SUCCESS }
                    .getOrDefault(false)
        }

    private val location = LocationSource(ctx, model, gms, gnssEvidence)
    private val heading = HeadingSource(ctx, model, gms)

    // The flags overload exists from Android 13; older phones only have the int one.
    @Suppress("DEPRECATION")
    private fun appEnabled(pkg: String): Boolean =
        runCatching {
            val pm = ctx.packageManager
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                pm.getApplicationInfo(pkg, PackageManager.ApplicationInfoFlags.of(0)).enabled
            } else {
                pm.getApplicationInfo(pkg, 0).enabled
            }
        }.getOrDefault(false)

    /** Start (or re-start with a new [rate]). Calling again with the same rate does nothing. */
    fun startLocation(rate: GpsPolicy.Rate) = location.startLocation(rate)

    /** A new game was opened: its first location start may ask for a cold-start fix again, and Play services is tried again. */
    fun allowColdStart() = location.allowColdStart()

    /** Stop location: listener, GNSS status callback, compass, any pending cold-start fix, and the once-per-session cold-start mark. */
    fun stopLocation() {
        location.stopLocation()
        stopGnss()
        stopHeading()
    }

    fun startSteps() {
        if (stepListener != null) return
        val sensor = sm.getDefaultSensor(Sensor.TYPE_STEP_COUNTER) ?: return Diag.warn(TAG, "no step counter on this device")
        val l =
            object : SensorEventListener {
                override fun onSensorChanged(e: SensorEvent) {
                    val total = e.values[0].toLong()
                    val eventMs = FixTime.wallMs(0L, e.timestamp, System.currentTimeMillis(), SystemClock.elapsedRealtimeNanos())
                    Diag.raw("rawsteps", RawLines.steps(total, eventMs))
                    model.onSteps(total, eventMs)
                }

                override fun onAccuracyChanged(
                    s: Sensor?,
                    a: Int,
                ) {
                    // The step counter's accuracy does not matter here.
                }
            }
        // Let the hardware batch readings (10 s, or 2 s in a zone where the filter wants the cadence).
        sm.registerListener(l, sensor, SensorManager.SENSOR_DELAY_NORMAL, if (fastSteps) FAST_STEP_BATCH_US else STEP_BATCH_US)
        stepListener = l
        Diag.info(TAG, "steps started")
    }

    fun stopSteps() {
        stepListener?.let { sm.unregisterListener(it) }
        stepListener = null
    }

    /** While in a zone: a `gnss` line every 10 s (no positions), and the chip's model and capabilities once per session. */
    @SuppressLint("MissingPermission")
    fun startGnss() {
        if (gnssCallback != null) return
        val cb =
            object : GnssStatus.Callback() {
                override fun onSatelliteStatusChanged(status: GnssStatus) {
                    val used = (0 until status.satelliteCount).count { status.usedInFix(it) }
                    gnssEvidence.onStatus(used, SystemClock.elapsedRealtime())
                    if (!gnssThrottle.due(System.currentTimeMillis())) return
                    val sats =
                        (0 until status.satelliteCount).map { i ->
                            Sat(
                                status.getConstellationType(i),
                                status.usedInFix(i),
                                status.getCn0DbHz(i),
                                status.getCarrierFrequencyHz(i).takeIf { status.hasCarrierFrequencyHz(i) },
                            )
                        }
                    Diag.info("gnss", "status", *GnssSummary.fields(sats).toList().toTypedArray())
                }
            }
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                lm.registerGnssStatusCallback(ctx.mainExecutor, cb)
            } else {
                @Suppress("DEPRECATION") // the Handler overload is the only one before Android 11
                lm.registerGnssStatusCallback(cb, Handler(Looper.getMainLooper()))
            }
            gnssCallback = cb
            gnssFailLogged = false
        }.onFailure {
            if (!gnssFailLogged) Diag.error(TAG, "gnss status failed", it)
            gnssFailLogged = true
        }
        if (!gnssHardwareLogged) {
            gnssHardwareLogged = true
            val model = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) lm.gnssHardwareModelName else null
            val caps = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) lm.gnssCapabilities.toString() else null
            Diag.info("gnss", "hardware", "model" to model, "capabilities" to caps)
        }
    }

    fun stopGnss() {
        gnssCallback?.let { lm.unregisterGnssStatusCallback(it) }
        gnssCallback = null
    }

    /**
     * While in a zone: the compass for the map arrow, every [periodUs] ([CompassRate]); re-registers when the period or the source
     * changes. [FusedHeadingWatch] picks Google's fused orientation (its own heading error, true north) or the rotation vector; the fused
     * one falls back to the rotation vector when it fails to start, goes silent or has no heading, until the visibility changes.
     */
    fun startHeading(
        periodUs: Int,
        onScreen: Boolean,
    ) = heading.startHeading(periodUs, onScreen)

    fun stopHeading() = heading.stopHeading()

    /** Steps every 2 s while in a zone (cadence for the filter), 10 s otherwise; re-registers when it changes. */
    fun setFastSteps(on: Boolean) {
        if (fastSteps == on) return
        fastSteps = on
        if (stepListener != null) {
            stopSteps()
            startSteps()
        }
    }
}
