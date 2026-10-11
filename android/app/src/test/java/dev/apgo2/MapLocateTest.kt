package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.maplibre.android.geometry.LatLng

class MapLocateTest {
    @Test fun zoomedOutItZoomsInToStreetLevel() {
        assertEquals(MapLocate.STREET_ZOOM, MapLocate.zoom(12.0), 0.0)
        assertEquals(MapLocate.STREET_ZOOM, MapLocate.zoom(0.0), 0.0)
    }

    @Test fun alreadyCloseItKeepsTheZoom() {
        assertEquals(MapLocate.STREET_ZOOM, MapLocate.zoom(MapLocate.STREET_ZOOM), 0.0)
        assertEquals(18.5, MapLocate.zoom(18.5), 0.0)
    }

    @Test fun streetLevelShowsPickupTokens() {
        assertEquals(16.0, MapLocate.STREET_ZOOM, 0.0)
    }

    @Test fun aLiveFixWins() {
        assertEquals(LatLng(1.0, 2.0), MapLocate.target(LatLng(1.0, 2.0), LatLng(3.0, 4.0)))
    }

    @Test fun pausedWithNoFixItGoesToTheLastPlace() {
        assertEquals(LatLng(3.0, 4.0), MapLocate.target(null, LatLng(3.0, 4.0)))
    }

    @Test fun nothingKnownMeansNowhereToGo() {
        assertNull(MapLocate.target(null, null))
    }
}
