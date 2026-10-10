package dev.apgo2

import android.location.Location
import android.os.Build
import uniffi.apgo_ffi.FixIn

private const val NANOS_PER_MS = 1_000_000L

// How old a last-known fix may be and still serve as a cold start.
private const val FRESH_MS = 120_000L

// A fix without an accuracy is unusable in the core (anything over 100 m is).
private const val NO_ACCURACY_M = 1_000.0

/** One location fix as the phone delivered it, every field the filter and the bench can use; `null` = the phone did not say. */
internal data class FixSample(
    val tMs: Long,
    val elapsedNs: Long,
    val lat: Double,
    val lon: Double,
    val accuracyM: Float?,
    val speedMps: Float?,
    val speedAccMps: Float?,
    val bearingDeg: Float?,
    val bearingAccDeg: Float?,
    val altitudeM: Double?,
    val verticalAccM: Float?,
    val provider: String,
    val mock: Boolean,
)

/** The fix as the core takes it. A mock fix stays a mock (the core drops it) unless the debug bench allows mocks. */
internal fun FixSample.toFixIn(mockAllowed: Boolean = false): FixIn =
    FixIn(
        tMs = tMs,
        lat = lat,
        lon = lon,
        accuracyM = accuracyM?.toDouble() ?: NO_ACCURACY_M,
        speedMps = speedMps?.toDouble(),
        speedAccMps = speedAccMps?.toDouble(),
        bearingDeg = bearingDeg?.toDouble(),
        bearingAccDeg = bearingAccDeg?.toDouble(),
        altitudeM = altitudeM,
        verticalAccM = verticalAccM?.toDouble(),
        provider = provider,
        mock = mock && !mockAllowed,
    )

/**
 * Mock-location fixes reach the filter only in a debuggable build with the bench flag file (`files/allow_mock`). The two gates are
 * this one (what the app sends: production passes the mock flag through) and the core's bench-only `LocParams.allow_mock` (which
 * rejects flagged mocks); do not add a third.
 */
internal object MockPolicy {
    fun allowed(
        debuggable: Boolean,
        flagFileExists: Boolean,
    ): Boolean = debuggable && flagFileExists
}

/** When a fix was taken. The fix's own clock, not the moment it reached the app (batched fixes arrive seconds late). */
internal object FixTime {
    /**
     * Wall time of a fix in ms: the monotonic `elapsedRealtimeNanos` of the fix measured back from now (immune to a wrong GPS or
     * wall clock), or `Location.time` when the phone did not fill in the elapsed time.
     */
    fun wallMs(
        fixTimeMs: Long,
        fixElapsedNs: Long,
        nowWallMs: Long,
        nowElapsedNs: Long,
    ): Long = if (fixElapsedNs > 0L) nowWallMs - (nowElapsedNs - fixElapsedNs) / NANOS_PER_MS else fixTimeMs

    /** Whether a fix taken at [fixElapsedNs] is at most 2 minutes old; a fix without an elapsed time has an unknown age (not fresh). */
    fun fresh(
        fixElapsedNs: Long,
        nowElapsedNs: Long,
    ): Boolean = fixElapsedNs > 0L && nowElapsedNs - fixElapsedNs <= FRESH_MS * NANOS_PER_MS

    /** A batch in the order the fixes were taken. */
    fun <T> oldestFirst(
        items: List<T>,
        elapsedNs: (T) -> Long,
    ): List<T> = items.sortedBy(elapsedNs)
}

/** The fields of the raw-track lines (`diag/raw/raw-NNNN.jsonl`, debug builds only), in the order the replay bench reads them. */
internal object RawLines {
    fun fix(s: FixSample): Map<String, Any?> =
        linkedMapOf(
            "tf" to s.tMs,
            "ert" to s.elapsedNs,
            "lat" to s.lat,
            "lon" to s.lon,
            "acc" to s.accuracyM,
            "spd" to s.speedMps,
            "spd_acc" to s.speedAccMps,
            "brg" to s.bearingDeg,
            "brg_acc" to s.bearingAccDeg,
            "alt" to s.altitudeM,
            "valt" to s.verticalAccM,
            "prov" to s.provider,
            "mock" to s.mock,
        )

    fun steps(
        total: Long,
        eventMs: Long,
    ): Map<String, Any?> = linkedMapOf("total" to total, "te" to eventMs)

    /**
     * A compass reading: `te` is the sensor event time on the fix clock (the replay bench reads it, never the log time); `err` only
     * when the phone gave its own heading error (fused orientation).
     */
    fun heading(
        azimuthDeg: Double,
        accuracy: String,
        pitchDeg: Double,
        rollDeg: Double,
        eventMs: Long,
        errorDeg: Double? = null,
    ): Map<String, Any?> =
        linkedMapOf<String, Any?>("te" to eventMs, "az" to azimuthDeg, "acc" to accuracy, "pitch" to pitchDeg, "roll" to rollDeg).apply {
            if (errorDeg != null) put("err", errorDeg)
        }

    fun state(
        presence: String,
        counting: Boolean,
        zone: String,
        appVisible: Boolean,
    ): Map<String, Any?> = linkedMapOf("presence" to presence, "counting" to counting, "zone" to zone, "app_visible" to appVisible)
}

/** Reads a [FixSample] off an Android [Location] (thin: the logic is in [FixTime]). */
internal object FixSamples {
    fun from(
        loc: Location,
        nowWallMs: Long,
        nowElapsedNs: Long,
    ): FixSample =
        FixSample(
            tMs = FixTime.wallMs(loc.time, loc.elapsedRealtimeNanos, nowWallMs, nowElapsedNs),
            elapsedNs = loc.elapsedRealtimeNanos,
            lat = loc.latitude,
            lon = loc.longitude,
            accuracyM = loc.accuracy.takeIf { loc.hasAccuracy() },
            speedMps = loc.speed.takeIf { loc.hasSpeed() },
            speedAccMps = loc.speedAccuracyMetersPerSecond.takeIf { loc.hasSpeedAccuracy() },
            bearingDeg = loc.bearing.takeIf { loc.hasBearing() },
            bearingAccDeg = loc.bearingAccuracyDegrees.takeIf { loc.hasBearingAccuracy() },
            altitudeM = loc.altitude.takeIf { loc.hasAltitude() },
            verticalAccM = loc.verticalAccuracyMeters.takeIf { loc.hasVerticalAccuracy() },
            provider = loc.provider ?: "?",
            mock = isMock(loc),
        )

    // isFromMockProvider is deprecated from Android 12, where isMock replaces it; older phones only have the old call.
    @Suppress("DEPRECATION")
    private fun isMock(loc: Location): Boolean = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) loc.isMock else loc.isFromMockProvider
}
