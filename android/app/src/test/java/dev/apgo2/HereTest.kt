package dev.apgo2

import dev.apgo2.presence.GeoFix
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.PositionOut

class HereTest {
    private val sim = LatLng(1.0, 1.0)
    private val est = LatLng(2.0, 2.0)
    private val raw = LatLng(3.0, 3.0)

    // Ruling T15-me: logic uses the estimate when a game is open, the raw fix otherwise; the simulator wins over both.
    @Test fun theSimulatorThenTheEstimateThenTheRawFix() {
        assertEquals(sim, Here.pick(sim, est, raw))
        assertEquals(est, Here.pick(null, est, raw))
        assertEquals("no game, or no estimate yet: the raw fix", raw, Here.pick(null, null, raw))
        assertNull(Here.pick(null, null, null))
    }

    // Adversarial review M3: game logic never takes the pin (bridged, predicted or stale) nor the raw fix.
    @Test fun logicTakesTheSimulatorThenTheAcceptedEstimateOnly() {
        assertEquals(sim, Here.logic(sim, est))
        assertEquals(est, Here.logic(null, est))
        assertNull("no accepted estimate yet: nowhere", Here.logic(null, null))
    }

    private fun pos(
        source: String,
        bridged: Boolean = source == "bridged",
    ) = PositionOut(9.0, 9.0, 2.0, 2.5, 7.0, 0.0, null, null, "none", false, 0.0, source, 1_500L, false, bridged)

    @Test fun theHomeOfferUsesTheEstimateOnlyWhenItIsFromGps() {
        val rawFix = GeoFix(3.0, 3.0, 12.0)
        assertEquals(GeoFix(2.0, 2.5, 7.0) to 1_500L, HomeOfferFix.choose(pos("gps"), rawFix, 40L))
        assertEquals(rawFix to 40L, HomeOfferFix.choose(pos("bridged"), rawFix, 40L))
        assertEquals(rawFix to 40L, HomeOfferFix.choose(pos("stale"), rawFix, 40L))
        assertEquals("no game", rawFix to 40L, HomeOfferFix.choose(null, rawFix, 40L))
    }
}
