package dev.apgo2

import android.location.Location
import dev.apgo2.ui.METERS_PER_DEGREE
import dev.apgo2.ui.MarkerSpec
import uniffi.apgo_ffi.PositionOut
import kotlin.math.cos
import kotlin.math.hypot

// A snap still goes through MapLibre's look-ahead animation: one frame, so its target time is never already in the past.
private const val SNAP_FRAME_MS = 16L
private const val GPS = "gps"
private const val BRIDGED = "bridged"
private const val STALE = "stale"

/** The estimate behind a pin and its age, to tell a new fix from the same one asked for again. */
internal data class FixMark(
    val lat: Double,
    val lon: Double,
    val ageMs: Long,
)

/**
 * What the map's location component shows: where, how sure (circle radius, 0 = none), which way, which image and how long to glide.
 * [fix] is the estimate behind it and [atMs] when it was made (both null for a pin straight from a raw fix).
 */
internal data class MePin(
    val lat: Double,
    val lon: Double,
    val accuracyM: Float,
    val bearingDeg: Float?,
    val spec: MarkerSpec.Me,
    val animateMs: Long,
    val fix: FixMark? = null,
    val atMs: Long? = null,
) {
    /** The pin as an Android location for `forceLocationUpdate(listOf(it), lookAheadUpdate = true)`, ending its glide on time. */
    fun toLocation(nowMs: Long): Location =
        Location("apgo").also { l ->
            l.latitude = lat
            l.longitude = lon
            l.accuracy = accuracyM
            bearingDeg?.let { l.bearing = it }
            l.time = MePins.glideEndMs(this, nowMs)
        }
}

/** Turns the core's `position()` into a pin (pure, unit-tested). */
internal object MePins {
    /** The longest glide: positions arrive about once a second. A refresh sooner glides for the gap since the last one. */
    const val GLIDE_MS = 1_000L
    private const val MIN_GLIDE_MS = 200L
    private const val SNAP_M = 50.0
    private const val MIN_CIRCLE_M = 3.0
    private val STATES = listOf(GPS, BRIDGED, STALE)

    /**
     * The pin for [p], gliding from [previous]. It jumps for the first pin, a jump over 50 m, and once per new fix that restarted the
     * filter: `snap` stays true until the next fix, so a new fix is told apart by the estimate's age dropping or the estimate moving.
     * A glide lasts the time since [previous] was made at [nowMs], 200 ms to 1 s (1 s when either time is unknown).
     */
    fun from(
        p: PositionOut,
        previous: MePin?,
        nowMs: Long? = null,
    ): MePin {
        val heading = p.headingSource != "none" && p.headingDeg != null
        val state = p.source.takeIf { it == BRIDGED || it == STALE } ?: GPS // "predicted" still shows as a solid dot
        val mark = FixMark(p.estLat, p.estLon, p.ageMs)
        val last = previous?.fix
        val newFix = last == null || mark.ageMs < last.ageMs || mark.lat != last.lat || mark.lon != last.lon
        val jump = previous == null || metersBetween(previous.lat, previous.lon, p.lat, p.lon) > SNAP_M
        return MePin(
            lat = p.lat,
            lon = p.lon,
            accuracyM = if (p.uncertaintyM < MIN_CIRCLE_M) 0f else p.uncertaintyM.toFloat(),
            bearingDeg = p.headingDeg?.toFloat()?.takeIf { heading },
            spec = MarkerSpec.Me(heading, state),
            animateMs = if ((p.snap && newFix) || jump) 0L else glideMs(previous.atMs, nowMs),
            fix = mark,
            atMs = nowMs,
        )
    }

    private fun glideMs(
        lastMs: Long?,
        nowMs: Long?,
    ): Long = if (lastMs == null || nowMs == null) GLIDE_MS else (nowMs - lastMs).coerceIn(MIN_GLIDE_MS, GLIDE_MS)

    /** When a pin's glide ends: MapLibre's look-ahead update animates from now until the location's time. */
    fun glideEndMs(
        pin: MePin,
        nowMs: Long,
    ): Long = nowMs + maxOf(pin.animateMs, SNAP_FRAME_MS)

    /** A pin straight from a raw fix (no game open, or the simulator). */
    @Suppress("FunctionNameMinLength") // reads as `MePins.at(lat, lon, accuracy)`, the plan's name
    fun at(
        lat: Double,
        lon: Double,
        accuracyM: Double?,
    ): MePin = MePin(lat, lon, accuracyM?.takeIf { it >= MIN_CIRCLE_M }?.toFloat() ?: 0f, null, MarkerSpec.Me(false, GPS), 0L)

    /** Every pin image the style needs. */
    fun specs(): List<MarkerSpec.Me> = listOf(false, true).flatMap { h -> STATES.map { MarkerSpec.Me(h, it) } }

    private fun metersBetween(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double = hypot((lat2 - lat1) * METERS_PER_DEGREE, (lon2 - lon1) * METERS_PER_DEGREE * cos(Math.toRadians(lat1)))
}
