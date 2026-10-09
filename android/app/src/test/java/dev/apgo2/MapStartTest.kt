package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.maplibre.android.geometry.LatLng

class MapStartTest {
    private val last = LatLng(51.5, -0.1)

    @Test fun nothingKnownMeansNoStart() {
        assertNull(MapStart.center(emptyList(), null))
    }

    @Test fun withNothingToFrameTheLastPlaceIsTheStart() {
        assertEquals(last, MapStart.center(emptyList(), last))
    }

    @Test fun oneThingToFrameIsTheStart() {
        assertEquals(LatLng(40.0, -111.0), MapStart.center(listOf(LatLng(40.0, -111.0)), last))
    }

    @Test fun severalThingsStartInTheMiddleOfTheirBox() {
        val c = MapStart.center(listOf(LatLng(40.0, -112.0), LatLng(41.0, -111.0), LatLng(40.2, -111.8)), last)!!
        assertEquals(40.5, c.latitude, 1e-9)
        assertEquals(-111.5, c.longitude, 1e-9)
    }

    @Test fun thingsAcrossTheAntimeridianStartBetweenThemTheShortWay() {
        val c = MapStart.center(listOf(LatLng(0.0, 179.0), LatLng(0.0, -177.0)), last)!!
        assertEquals(-179.0, c.longitude, 1e-9)
    }
}
