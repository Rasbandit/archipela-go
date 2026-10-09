package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GnssTest {
    // Adversarial review I2: a fused fix while the GNSS chip reports no satellite used in a fix is a Wi-Fi or cell position.
    @Test fun aFusedFixWithNoSatelliteUsedIsNetwork() {
        val g = GnssEvidence()
        assertEquals("no GnssStatus yet: trust fused", "fused", g.providerOf("fused", 1_000L))
        g.onStatus(used = 0, elapsedMs = 1_000L)
        assertEquals("network", g.providerOf("fused", 2_000L))
        assertTrue(g.networkOnly(2_000L))
        assertEquals("a status over 30 s old is no evidence", "fused", g.providerOf("fused", 31_001L))
        g.onStatus(used = 5, elapsedMs = 40_000L)
        assertEquals("fused", g.providerOf("fused", 41_000L))
        assertFalse(g.networkOnly(41_000L))
        assertEquals("only fused fixes are retagged", "gps", g.providerOf("gps", 41_000L))
    }

    @Test fun carrierFrequenciesMapToBands() {
        assertEquals("L1", GnssBands.of(1_575.42e6f)) // GPS L1, Galileo E1
        assertEquals("L1", GnssBands.of(1_561.098e6f)) // BeiDou B1
        assertEquals("L1", GnssBands.of(1_602.0e6f)) // GLONASS G1
        assertEquals("L5", GnssBands.of(1_176.45e6f)) // GPS L5, Galileo E5a
        assertEquals("E5b", GnssBands.of(1_207.14e6f))
        assertEquals("L2", GnssBands.of(1_227.6e6f))
        assertEquals("E6", GnssBands.of(1_278.75e6f))
        assertEquals("other", GnssBands.of(0f))
    }

    @Test fun aSummaryCountsSatellitesByConstellationAndSeesDualFrequency() {
        val sats =
            listOf(
                Sat(constellation = 1, used = true, cn0 = 40f, carrierHz = 1_575.42e6f),
                Sat(constellation = 1, used = true, cn0 = 38f, carrierHz = 1_176.45e6f),
                Sat(constellation = 6, used = false, cn0 = 30f, carrierHz = 1_575.42e6f),
                Sat(constellation = 6, used = true, cn0 = 36f, carrierHz = null),
                Sat(constellation = 5, used = false, cn0 = 20f, carrierHz = 1_561.098e6f),
            )
        val f = GnssSummary.fields(sats)
        assertEquals(5, f["in_view"])
        assertEquals(3, f["used"])
        assertEquals("beidou=0/1,galileo=1/2,gps=2/2", f["by_constellation"])
        assertEquals(36.0, f["cn0_top4"] as Double, 1e-9) // 40, 38, 36, 30
        assertEquals("L1,L5", f["bands"])
        assertEquals(true, f["dual_freq"])
    }

    @Test fun anEmptySkyIsAllZeros() {
        val f = GnssSummary.fields(emptyList())
        assertEquals(0, f["in_view"])
        assertEquals(0.0, f["cn0_top4"] as Double, 0.0)
        assertEquals(false, f["dual_freq"])
    }
}
