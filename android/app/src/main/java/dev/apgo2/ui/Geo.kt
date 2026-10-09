package dev.apgo2.ui

import org.maplibre.android.geometry.LatLng
import kotlin.math.cos
import kotlin.math.sin

/** Metres in one degree of latitude (and of longitude at the equator). */
internal const val METERS_PER_DEGREE = 111_195.0

internal const val METERS_PER_KM = 1000
private const val RING_POINTS = 48
private const val DEGREES_PER_RING_POINT = 360.0 / RING_POINTS

/** Points (lat, lon) on a circle of [radiusM] metres around a centre, for drawing it as a polygon. */
internal fun circleRing(
    lat: Double,
    lon: Double,
    radiusM: Double,
): List<Pair<Double, Double>> =
    (0 until RING_POINTS).map { i ->
        val a = Math.toRadians(i * DEGREES_PER_RING_POINT)
        val dLat = radiusM * cos(a) / METERS_PER_DEGREE
        val dLon = radiusM * sin(a) / (METERS_PER_DEGREE * cos(Math.toRadians(lat)))
        lat + dLat to lon + dLon
    }

/** A polygon needs this many corners to be an area. */
internal const val MIN_POLYGON_CORNERS = 3

/** The four points a circle reaches due north, south, east and west, to fit it in view. */
internal fun circleExtremes(
    center: LatLng,
    radiusM: Double,
): List<LatLng> {
    val dLat = radiusM / METERS_PER_DEGREE
    val dLon = radiusM / (METERS_PER_DEGREE * cos(Math.toRadians(center.latitude)))
    return listOf(
        LatLng(center.latitude + dLat, center.longitude),
        LatLng(center.latitude - dLat, center.longitude),
        LatLng(center.latitude, center.longitude + dLon),
        LatLng(center.latitude, center.longitude - dLon),
    )
}
