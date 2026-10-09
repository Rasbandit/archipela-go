package dev.apgo2

import org.maplibre.android.geometry.LatLng

private const val HALF_TURN = 180.0
private const val TURN = 360.0

/** Where a new map's camera starts, so its first frame is already near the action and never the whole world. */
internal object MapStart {
    /**
     * The middle of [points] (what the map will frame), else [fallback] (the last place), else null when nothing is known. Points
     * across the antimeridian are measured the short way.
     */
    fun center(
        points: List<LatLng>,
        fallback: LatLng?,
    ): LatLng? {
        if (points.isEmpty()) return fallback
        val lats = points.map { it.latitude }
        val around = points[0].longitude
        val lons = points.map { unwrap(it.longitude, around) }
        return LatLng((lats.min() + lats.max()) / 2, normal((lons.min() + lons.max()) / 2))
    }

    // [lon] moved by whole turns to within half a turn of [around].
    private fun unwrap(
        lon: Double,
        around: Double,
    ) = around + (lon - around + HALF_TURN).mod(TURN) - HALF_TURN

    private fun normal(lon: Double) = if (lon in -HALF_TURN..HALF_TURN) lon else unwrap(lon, 0.0)
}
