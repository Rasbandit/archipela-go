package dev.apgo2

import android.hardware.SensorManager
import android.view.Surface
import uniffi.apgo_ffi.HeadingIn

// The rotation vector at 5 Hz while the player looks at the map, once a second otherwise.
private const val ON_SCREEN_PERIOD_US = 200_000
private const val OFF_SCREEN_PERIOD_US = 1_000_000
private const val UNRELIABLE = "unreliable"

/** `SensorManager.SENSOR_STATUS_ACCURACY_*` (3 high, 2 medium, 1 low, 0 unreliable) as the core's names. */
internal object CompassAccuracies {
    fun name(sensorStatus: Int): String =
        when (sensorStatus) {
            SensorManager.SENSOR_STATUS_ACCURACY_HIGH -> "high"
            SensorManager.SENSOR_STATUS_ACCURACY_MEDIUM -> "medium"
            SensorManager.SENSOR_STATUS_ACCURACY_LOW -> "low"
            else -> UNRELIABLE
        }

    /**
     * The accuracy of one reading: the event's own when it is usable, else the last `onAccuracyChanged` value (some phones never
     * call it, others leave the event's field unreliable).
     */
    fun effective(
        eventAccuracy: Int,
        lastChanged: Int,
    ): Int = if (eventAccuracy > SensorManager.SENSOR_STATUS_UNRELIABLE) eventAccuracy else maxOf(eventAccuracy, lastChanged)
}

/** The device axes that become the world's X and Y for `SensorManager.remapCoordinateSystem`, so north follows a turned screen. */
internal object CompassAxes {
    fun forRotation(surfaceRotation: Int): Pair<Int, Int> =
        when (surfaceRotation) {
            Surface.ROTATION_90 -> SensorManager.AXIS_Y to SensorManager.AXIS_MINUS_X
            Surface.ROTATION_180 -> SensorManager.AXIS_MINUS_X to SensorManager.AXIS_MINUS_Y
            Surface.ROTATION_270 -> SensorManager.AXIS_MINUS_Y to SensorManager.AXIS_X
            else -> SensorManager.AXIS_X to SensorManager.AXIS_Y
        }
}

/** How often the compass reads: fast while the map is on screen, slow with the screen off or the app away. */
internal object CompassRate {
    fun periodUs(
        screenOn: Boolean,
        appVisible: Boolean,
    ): Int = if (screenOn && appVisible) ON_SCREEN_PERIOD_US else OFF_SCREEN_PERIOD_US
}

/**
 * Which compass: Google's fused orientation with Play services while the map is on screen (it only delivers while the app is in the
 * foreground), else the rotation vector, which keeps the pocket compass going with the screen off.
 */
internal object CompassSource {
    fun fused(
        gms: Boolean,
        onScreen: Boolean,
    ): Boolean = gms && onScreen
}

// The fused compass is given up after this long without a reading, or this long with no heading (error 180 or more).
private const val FUSED_SILENT_MS = 3_000L
private const val FUSED_NO_HEADING_MS = 5_000L
private const val NO_HEADING_ERROR_DEG = 180.0

/**
 * Whether to use Google's fused orientation now, and when to give it up for the rotation vector: it failed to start, delivered nothing
 * for 3 s, or reported no heading (error >= 180) for 5 s. Given up stays given up until the visibility changes, so a failing fused
 * compass is not re-asked (and re-logged) on every location change. Times are any one monotonic clock in milliseconds.
 */
internal class FusedHeadingWatch {
    private var onScreen: Boolean? = null
    private var gaveUp = false
    private var lastMs = 0L
    private var noHeadingSinceMs: Long? = null

    /** Whether the fused compass should run for this state; a visibility change clears an earlier give-up. */
    fun wanted(
        gms: Boolean,
        onScreen: Boolean,
    ): Boolean {
        if (this.onScreen != onScreen) gaveUp = false
        this.onScreen = onScreen
        return CompassSource.fused(gms, onScreen) && !gaveUp
    }

    /** The fused compass was (re-)asked at [nowMs]. */
    fun started(nowMs: Long) {
        lastMs = nowMs
        noHeadingSinceMs = null
    }

    /** A fused reading arrived at [nowMs] with its heading error. */
    fun reading(
        nowMs: Long,
        errorDeg: Double,
    ) {
        lastMs = nowMs
        val noHeading = errorDeg.isNaN() || errorDeg >= NO_HEADING_ERROR_DEG
        noHeadingSinceMs = if (noHeading) noHeadingSinceMs ?: nowMs else null
    }

    /** The fused compass failed to start. */
    fun failed() {
        gaveUp = true
    }

    /** Why the fused compass should be given up at [nowMs] ("silent" or "no_heading"), or null to keep it. */
    fun fallBack(nowMs: Long): String? {
        val since = noHeadingSinceMs
        val why =
            when {
                nowMs - lastMs >= FUSED_SILENT_MS -> "silent"
                since != null && nowMs - since >= FUSED_NO_HEADING_MS -> "no_heading"
                else -> null
            }
        if (why != null) gaveUp = true
        return why
    }
}

// A fused heading error is half of a 95 % cone (about two sigma); 180 means no idea. The bands match the core's sigmas.
private const val HIGH_ERROR_DEG = 30.0
private const val MEDIUM_ERROR_DEG = 60.0
private const val LOW_ERROR_DEG = 90.0
private const val FULL_CIRCLE_DEG = 360.0

/**
 * One compass reading for the core. [trueNorth] when the azimuth is already from true north (Google's fused orientation), else it is
 * magnetic (the rotation vector) and gets the declination here. [errorDeg] is the phone's own heading error when it gives one.
 */
internal data class CompassReading(
    val azimuthDeg: Double,
    val trueNorth: Boolean,
    val accuracy: String,
    val errorDeg: Double?,
    val pitchDeg: Double,
    val rollDeg: Double,
    val eventMs: Long,
)

internal object CompassReadings {
    /** The core's reading: a magnetic azimuth turned by [declinationDeg] (east positive), a true-north one as it is. */
    fun headingIn(
        r: CompassReading,
        declinationDeg: Double,
    ): HeadingIn {
        val az = if (r.trueNorth) r.azimuthDeg else (r.azimuthDeg + declinationDeg).mod(FULL_CIRCLE_DEG)
        return HeadingIn(
            azimuthDeg = az,
            accuracy = r.accuracy,
            pitchDeg = r.pitchDeg,
            rollDeg = r.rollDeg,
            tMs = r.eventMs,
            errorDeg = r.errorDeg,
        )
    }
}

/**
 * Google's Fused Orientation Provider (Play services): `headingDegrees` already follows the turned screen and is from true north when
 * Play services knows the declination (magnetic otherwise; it does not say which), with `headingErrorDegrees` as its own error.
 */
internal object FusedCompass {
    /** The accuracy band of a heading error, for the raw track and the band field; the core uses the error itself. */
    fun accuracyName(errorDeg: Double): String =
        when {
            errorDeg.isNaN() || errorDeg < 0.0 -> UNRELIABLE
            errorDeg <= HIGH_ERROR_DEG -> "high"
            errorDeg <= MEDIUM_ERROR_DEG -> "medium"
            errorDeg <= LOW_ERROR_DEG -> "low"
            else -> UNRELIABLE
        }

    /** One fused sample (heading and error as the API gives them, the tilt from its attitude) as a reading. */
    fun reading(
        headingDeg: Float,
        errorDeg: Float,
        pitchDeg: Double,
        rollDeg: Double,
        eventMs: Long,
    ): CompassReading {
        val err = errorDeg.toDouble()
        return CompassReading(headingDeg.toDouble(), trueNorth = true, accuracyName(err), err, pitchDeg, rollDeg, eventMs)
    }
}

private const val HALF_CIRCLE_DEG = 180.0
private const val COMPARE_SETTLE_MS = 200L

/**
 * The device check "is the fused heading from true or magnetic north": one fused azimuth against the rotation vector's magnetic one
 * read at the same moment, raw and corrected by the declination. A diff_true near 0 means true north; a diff_mag near 0 magnetic.
 */
internal object HeadingCompare {
    /** Compared only with the screen upright: both azimuths then follow the same axis, with no remap in between. */
    fun upright(surfaceRotation: Int): Boolean = surfaceRotation == Surface.ROTATION_0

    /** A rotation-vector event to compare with: not the first one (often the sensor's last state from before), unless 200 ms passed. */
    fun settled(
        eventNumber: Int,
        sinceRegisterMs: Long,
    ): Boolean = eventNumber >= 2 || sinceRegisterMs >= COMPARE_SETTLE_MS

    fun fields(
        fusedAzDeg: Double,
        rotationMagAzDeg: Double,
        declinationDeg: Double,
    ): List<Pair<String, Any?>> {
        val mag = rotationMagAzDeg.mod(FULL_CIRCLE_DEG)
        val trueAz = (mag + declinationDeg).mod(FULL_CIRCLE_DEG)
        return listOf(
            "fop_az" to fusedAzDeg,
            "rv_mag_az" to mag,
            "declination" to declinationDeg,
            "rv_true_az" to trueAz,
            "diff_true" to signedDiff(fusedAzDeg, trueAz),
            "diff_mag" to signedDiff(fusedAzDeg, mag),
        )
    }

    // a - b the short way round, in [-180, 180).
    private fun signedDiff(
        a: Double,
        b: Double,
    ): Double = (a - b + HALF_CIRCLE_DEG).mod(FULL_CIRCLE_DEG) - HALF_CIRCLE_DEG
}
