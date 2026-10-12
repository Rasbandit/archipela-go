package dev.apgo2

import dev.apgo2.ui.MarkerSpec
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MePinRedrawTest {
    // About 1 m of latitude.
    private val oneMetreLat = 1.0 / 111_320.0

    private fun pin(
        lat: Double = 40.0,
        lon: Double = -111.0,
        accuracy: Float = 6f,
        bearing: Float? = 90f,
        spec: MarkerSpec.Me = MarkerSpec.Me(true, "gps"),
        atMs: Long = 0L,
    ) = MePin(lat, lon, accuracy, bearing, spec, 1_000L, null, atMs)

    private fun differs(
        old: MePin?,
        new: MePin,
    ) = MePins.meaningfullyDiffers(old, new)

    @Test fun theFirstPinIsAlwaysDrawn() = assertTrue(differs(null, pin()))

    @Test fun theSamePinAskedForAgainIsSkipped() = assertFalse(differs(pin(atMs = 0L), pin(atMs = 1_000L)))

    @Test fun movingUnderAMetreIsSkippedAndAMetreOrMoreIsDrawn() {
        assertFalse(differs(pin(), pin(lat = 40.0 + 0.9 * oneMetreLat)))
        assertTrue(differs(pin(), pin(lat = 40.0 + 1.1 * oneMetreLat)))
    }

    @Test fun eastWestMovementCountsInMetresAtThisLatitude() {
        // At 60 degrees a degree of longitude is half as long: 1.5 m of latitude-degrees is only 0.75 m there.
        val lon = 1.5 * oneMetreLat
        assertFalse(differs(pin(lat = 60.0), pin(lat = 60.0, lon = -111.0 + lon)))
    }

    @Test fun turningUnderFiveDegreesIsSkippedAndFiveOrMoreIsDrawn() {
        assertFalse(differs(pin(bearing = 90f), pin(bearing = 94f)))
        assertTrue(differs(pin(bearing = 90f), pin(bearing = 95f)))
    }

    @Test fun turningAcrossNorthIsMeasuredTheShortWay() {
        assertFalse(differs(pin(bearing = 358f), pin(bearing = 2f)))
        assertTrue(differs(pin(bearing = 357f), pin(bearing = 3f)))
    }

    @Test fun gainingOrLosingTheArrowIsDrawn() {
        assertTrue(differs(pin(bearing = null), pin(bearing = 90f)))
        assertTrue(differs(pin(bearing = 90f), pin(bearing = null)))
    }

    @Test fun accuracyChangingUnderTwoMetresIsSkippedAndTwoOrMoreIsDrawn() {
        assertFalse(differs(pin(accuracy = 6f), pin(accuracy = 7.9f)))
        assertTrue(differs(pin(accuracy = 6f), pin(accuracy = 8f)))
        assertTrue(differs(pin(accuracy = 6f), pin(accuracy = 0f)))
    }

    @Test fun aNewImageIsAlwaysDrawn() {
        assertTrue(differs(pin(), pin(spec = MarkerSpec.Me(true, "stale"))))
    }
}
