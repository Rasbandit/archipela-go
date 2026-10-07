package dev.apgo2.ui

import kotlin.math.cos
import kotlin.math.sin

/** Points (lat, lon) on a circle of [radiusM] metres around a centre, for drawing it as a polygon. */
fun circleRing(lat: Double, lon: Double, radiusM: Double): List<Pair<Double, Double>> =
    (0 until 48).map { i ->
        val a = Math.toRadians(i * 7.5)
        val dLat = radiusM * cos(a) / 111_195.0
        val dLon = radiusM * sin(a) / (111_195.0 * cos(Math.toRadians(lat)))
        (lat + dLat) to (lon + dLon)
    }
