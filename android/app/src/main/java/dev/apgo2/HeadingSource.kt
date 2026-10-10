package dev.apgo2

import android.content.Context
import android.content.pm.ApplicationInfo
import android.hardware.GeomagneticField
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.hardware.display.DisplayManager
import android.location.Location
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.Display
import android.view.Surface
import androidx.core.content.ContextCompat
import com.google.android.gms.location.DeviceOrientationListener
import com.google.android.gms.location.DeviceOrientationRequest
import com.google.android.gms.location.LocationServices

// Compass readings may be batched up to 1 s ([PinFeed.onHeading] passes at most 2 Hz on to the core).
private const val HEADING_BATCH_US = 1_000_000
private const val ROTATION_MATRIX_SIZE = 9
private const val ORIENTATION_SIZE = 3
private const val TAG = SENSORS_TAG
private const val FUSED_CHECK_MS = 1_000L

// The heading compare waits for a fused heading at least this good (the "medium" band), so it compares north, not noise.
private const val COMPARE_MAX_ERROR_DEG = 60.0

/**
 * The compass for the map arrow: Google's fused orientation or the rotation vector ([FusedHeadingWatch]), and the debug-only
 * heading compare. Part of [Sensors].
 */
internal class HeadingSource(
    private val ctx: Context,
    private val model: AppModel,
    gms: Lazy<Boolean>,
) {
    private val sm = ctx.getSystemService(Context.SENSOR_SERVICE) as SensorManager
    private var headingListener: SensorEventListener? = null
    private var headingAccuracy = SensorManager.SENSOR_STATUS_UNRELIABLE
    private var headingPeriodUs = 0
    private var headingMissingLogged = false
    private var fusedListener: DeviceOrientationListener? = null
    private var headingFused = false

    // The compass is wanted (it may be running nothing: no sensor, or the fused one failed and is remembered as failed).
    private var headingOn = false
    private val fusedWatch = FusedHeadingWatch()
    private val headingHandler = Handler(Looper.getMainLooper())
    private val fusedCheck = Runnable { checkFused() }
    private val debuggable by lazy { ctx.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0 }
    private var headingCompared = false
    private var compareListener: SensorEventListener? = null
    private var lastFusedAz = 0.0
    private var lastFusedError = Double.NaN
    private val orientationClient by lazy { LocationServices.getFusedOrientationProviderClient(ctx) }
    private val rot = FloatArray(ROTATION_MATRIX_SIZE)
    private val screen = FloatArray(ROTATION_MATRIX_SIZE)
    private val ori = FloatArray(ORIENTATION_SIZE)
    private val displays = ctx.getSystemService(DisplayManager::class.java)
    private val display: Display? by lazy { displays.getDisplay(Display.DEFAULT_DISPLAY) }

    // Detected by package and by Play services saying it is usable ([Sensors]).
    private val gms: Boolean by gms

    // Context.getMainExecutor is Android 9+; this works from 8.
    private val mainExecutor by lazy { ContextCompat.getMainExecutor(ctx) }

    /**
     * While in a zone: the compass for the map arrow, every [periodUs] ([CompassRate]); re-registers when the period or the source
     * changes. [FusedHeadingWatch] picks Google's fused orientation (its own heading error, true north) or the rotation vector; the fused
     * one falls back to the rotation vector when it fails to start, goes silent or has no heading, until the visibility changes.
     */
    fun startHeading(
        periodUs: Int,
        onScreen: Boolean,
    ) {
        val fused = fusedWatch.wanted(gms, onScreen)
        if (headingOn && headingPeriodUs == periodUs && headingFused == fused) return
        stopHeading()
        headingOn = true
        headingPeriodUs = periodUs
        headingFused = fused
        if (fused) startFusedHeading(periodUs) else startRotationHeading(periodUs)
    }

    private fun startFusedHeading(periodUs: Int) {
        val l =
            DeviceOrientationListener { o ->
                fusedWatch.reading(SystemClock.elapsedRealtime(), o.headingErrorDegrees.toDouble())
                compareHeadingOnce(o.headingDegrees.toDouble(), o.headingErrorDegrees.toDouble())
                val (pitch, roll) = tilt(o.attitude)
                val eventMs = FixTime.wallMs(0L, o.elapsedRealtimeNs, System.currentTimeMillis(), SystemClock.elapsedRealtimeNanos())
                model.pins.onHeading(FusedCompass.reading(o.headingDegrees, o.headingErrorDegrees, pitch, roll, eventMs))
            }
        fusedListener = l
        fusedWatch.started(SystemClock.elapsedRealtime())
        headingHandler.postDelayed(fusedCheck, FUSED_CHECK_MS)
        val request = DeviceOrientationRequest.Builder(periodUs.toLong()).build()
        orientationClient
            .requestOrientationUpdates(request, mainExecutor, l)
            .addOnSuccessListener { Diag.info(TAG, "heading started", "period_us" to periodUs, "source" to FUSED_SOURCE) }
            .addOnFailureListener { e ->
                // Still wanted (not stopped or replaced meanwhile): use the plain rotation vector instead.
                if (fusedListener === l) {
                    fusedWatch.failed()
                    fallBackToRotation("failed", e.toString())
                }
            }
    }

    // Every second while the fused compass runs: give it up when it went silent or has no heading.
    private fun checkFused() {
        if (fusedListener == null) return
        val why = fusedWatch.fallBack(SystemClock.elapsedRealtime())
        if (why == null) headingHandler.postDelayed(fusedCheck, FUSED_CHECK_MS) else fallBackToRotation(why, null)
    }

    private fun fallBackToRotation(
        reason: String,
        error: String?,
    ) {
        Diag.warn(TAG, "fused heading fell back", "reason" to reason, "error" to error)
        headingHandler.removeCallbacks(fusedCheck)
        fusedListener?.let { orientationClient.removeOrientationUpdates(it) }
        fusedListener = null
        headingFused = false
        startRotationHeading(headingPeriodUs)
    }

    private fun startRotationHeading(periodUs: Int) {
        val sensor = sm.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)
        if (sensor == null) {
            if (!headingMissingLogged) Diag.warn(TAG, "no rotation vector sensor")
            headingMissingLogged = true
            return
        }
        val l =
            object : SensorEventListener {
                override fun onSensorChanged(e: SensorEvent) {
                    val (az, pitch, roll) = orientation(e.values)
                    val eventMs = FixTime.wallMs(0L, e.timestamp, System.currentTimeMillis(), SystemClock.elapsedRealtimeNanos())
                    val acc = CompassAccuracies.name(CompassAccuracies.effective(e.accuracy, headingAccuracy))
                    model.pins.onHeading(CompassReading(az, trueNorth = false, acc, errorDeg = null, pitch, roll, eventMs))
                }

                override fun onAccuracyChanged(
                    s: Sensor?,
                    a: Int,
                ) {
                    headingAccuracy = a
                }
            }
        sm.registerListener(l, sensor, periodUs, HEADING_BATCH_US)
        headingListener = l
        Diag.info(TAG, "heading started", "period_us" to periodUs, "source" to "rotation_vector")
    }

    // Azimuth (magnetic, where the top of the turned screen points), pitch and roll in degrees of a rotation vector.
    private fun orientation(rotationVector: FloatArray): Triple<Double, Double, Double> {
        SensorManager.getRotationMatrixFromVector(rot, rotationVector)
        val (x, y) = CompassAxes.forRotation(screenRotation())
        SensorManager.remapCoordinateSystem(rot, x, y, screen)
        SensorManager.getOrientation(screen, ori)
        val (az, pitch, roll) = ori.map { Math.toDegrees(it.toDouble()) }
        return Triple(az, pitch, roll)
    }

    private fun screenRotation(): Int = display?.rotation ?: Surface.ROTATION_0

    // A fused attitude is a scalar-last quaternion, which `getRotationMatrixFromVector` reads as is.
    private fun tilt(attitude: FloatArray): Pair<Double, Double> = orientation(attitude).let { (_, pitch, roll) -> pitch to roll }

    // Debug builds, once per session: a settled fused heading against the rotation vector's (one reading), so a diag pull shows whether
    // the fused azimuth is from true or magnetic north ([HeadingCompare]).
    private fun compareHeadingOnce(
        fusedAzDeg: Double,
        fusedErrorDeg: Double,
    ) {
        lastFusedAz = fusedAzDeg
        lastFusedError = fusedErrorDeg
        val wanted = debuggable && !headingCompared && compareListener == null && HeadingCompare.upright(screenRotation())
        val at = model.realLoc
        if (!wanted || !(fusedErrorDeg <= COMPARE_MAX_ERROR_DEG) || at == null) return
        val sensor = sm.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)
        if (sensor == null) headingCompared = true else startCompare(sensor, at)
    }

    private fun startCompare(
        sensor: Sensor,
        at: Location,
    ) {
        val registeredMs = SystemClock.elapsedRealtime()
        val l =
            object : SensorEventListener {
                private var events = 0

                override fun onSensorChanged(e: SensorEvent) {
                    events += 1
                    if (!HeadingCompare.settled(events, SystemClock.elapsedRealtime() - registeredMs)) return
                    val rotation = screenRotation()
                    // Turned meanwhile: drop this one and wait for the next settled upright fused reading.
                    if (!HeadingCompare.upright(rotation)) return stopCompare()
                    val (az, _, _) = orientation(e.values)
                    val decl = GeomagneticField(at.latitude.toFloat(), at.longitude.toFloat(), 0f, System.currentTimeMillis()).declination
                    val fields =
                        listOf("fop_err" to lastFusedError, "rotation" to rotation) +
                            HeadingCompare.fields(lastFusedAz, az, decl.toDouble())
                    Diag.info(TAG, "heading compare", *fields.toTypedArray())
                    headingCompared = true
                    stopCompare()
                }

                override fun onAccuracyChanged(
                    s: Sensor?,
                    a: Int,
                ) {
                    // One reading is enough; its accuracy is not part of the compare.
                }
            }
        compareListener = l
        sm.registerListener(l, sensor, SensorManager.SENSOR_DELAY_UI)
    }

    private fun stopCompare() {
        compareListener?.let { sm.unregisterListener(it) }
        compareListener = null
    }

    fun stopHeading() {
        headingOn = false
        stopCompare()
        headingHandler.removeCallbacks(fusedCheck)
        val fused = fusedListener
        val l = headingListener
        if (fused == null && l == null) return
        fused?.let { orientationClient.removeOrientationUpdates(it) }
        l?.let { sm.unregisterListener(it) }
        fusedListener = null
        headingListener = null
        Diag.info(TAG, "heading stopped")
    }
}
