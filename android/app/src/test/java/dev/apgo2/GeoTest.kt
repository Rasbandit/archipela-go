package dev.apgo2

import dev.apgo2.presence.GeoFix
import org.junit.Assert.assertEquals
import org.junit.Test

class GeoTest {
    private val home = GeoFix(lat = 47.0, lon = 8.0, accuracyM = 0.0)

    @Test fun distanceIsMeasuredInMetres() {
        assertEquals(0.0, distanceM(home, home), 1e-9)
        // 1 m north is about 1 / 111 195 degrees of latitude.
        assertEquals(100.0, distanceM(home, GeoFix(home.lat + 100.0 / 111_195.0, home.lon, 0.0)), 0.1)
    }
}
