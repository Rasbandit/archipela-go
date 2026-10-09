package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.maplibre.android.geometry.LatLng
import kotlin.math.cos
import kotlin.math.hypot

class GeoTest {
    private val eps = 1e-9

    // Flat-earth distance in metres, the same approximation the production code makes.
    private fun metres(
        lat: Double,
        lon: Double,
        p: Pair<Double, Double>,
    ) = hypot((p.first - lat) * METERS_PER_DEGREE, (p.second - lon) * METERS_PER_DEGREE * cos(Math.toRadians(lat)))

    @Test fun ringHas48PointsAllOnTheRadius() {
        val ring = circleRing(40.0, -111.0, 500.0)
        assertEquals(48, ring.size)
        ring.forEach { assertEquals(500.0, metres(40.0, -111.0, it), 1e-6) }
        assertEquals("points are distinct", 48, ring.toSet().size)
    }

    @Test fun ringStartsDueNorthAndGoesClockwise() {
        val ring = circleRing(0.0, 0.0, METERS_PER_DEGREE)
        assertEquals(1.0, ring[0].first, eps)
        assertEquals(0.0, ring[0].second, eps)
        assertEquals("quarter turn is due east", 1.0, ring[12].second, eps)
        assertEquals(0.0, ring[12].first, eps)
    }

    @Test fun ringWidensInLongitudeAwayFromTheEquator() {
        val equator = circleRing(0.0, 0.0, 1000.0)[12].second
        val north = circleRing(60.0, 0.0, 1000.0)[12].second
        assertEquals("cos(60) halves a degree of longitude", 2 * equator, north, 1e-9)
    }

    @Test fun zeroRadiusCollapsesToTheCentre() {
        circleRing(10.0, 20.0, 0.0).forEach {
            assertEquals(10.0, it.first, eps)
            assertEquals(20.0, it.second, eps)
        }
    }

    @Test fun extremesAreNorthSouthEastWest() {
        val c = LatLng(45.0, 7.0)
        val x = circleExtremes(c, 2000.0)
        assertEquals(4, x.size)
        val (n, s) = x
        val (e, w) = x.drop(2)
        val dLat = 2000.0 / METERS_PER_DEGREE
        assertEquals(45.0 + dLat, n.latitude, eps)
        assertEquals(45.0 - dLat, s.latitude, eps)
        assertEquals(7.0, n.longitude, eps)
        assertEquals(7.0, s.longitude, eps)
        assertEquals(45.0, e.latitude, eps)
        assertEquals(45.0, w.latitude, eps)
        assertTrue(e.longitude > 7.0 && w.longitude < 7.0)
        assertEquals("symmetric about the centre", 7.0 - e.longitude, w.longitude - 7.0, eps)
        assertEquals(dLat / cos(Math.toRadians(45.0)), e.longitude - 7.0, eps)
    }
}
