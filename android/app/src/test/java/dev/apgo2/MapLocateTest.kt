package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

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
}
