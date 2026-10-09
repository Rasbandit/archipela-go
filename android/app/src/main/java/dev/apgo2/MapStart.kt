package dev.apgo2

import org.maplibre.android.geometry.LatLng

/** Where a new map's camera starts, so its first frame is already near the action and never the whole world. */
internal object MapStart {
    /** The middle of [points] (what the map will frame), else [fallback] (the last place), else null when nothing is known. */
    fun center(
        points: List<LatLng>,
        fallback: LatLng?,
    ): LatLng? {
        if (points.isEmpty()) return fallback
        val lats = points.map { it.latitude }
        val lons = points.map { it.longitude }
        return LatLng((lats.min() + lats.max()) / 2, (lons.min() + lons.max()) / 2)
    }
}
