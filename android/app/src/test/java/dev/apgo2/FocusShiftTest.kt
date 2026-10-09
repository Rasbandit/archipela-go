package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class FocusShiftTest {
    // A 1000 x 2000 map with 100 covered at the top and 600 at the bottom, kept 20 clear of every edge.
    private val view = FocusShift.View(width = 1000f, height = 2000f, top = 100f, bottom = 600f, margin = 20f)

    private fun shift(
        x: Float,
        y: Float,
        above: Float = 300f,
        below: Float = 50f,
    ) = FocusShift.needed(x, y, above, below, view)

    @Test fun somethingAlreadyInViewDoesNotMove() {
        assertEquals(0f to 0f, shift(500f, 800f))
    }

    @Test fun aCalloutCutOffAtTheTopScrollsJustEnoughToShowIt() {
        // The callout's top is at 300 - 300 = 0; it must be at 100 + 20.
        assertEquals(0f to -120f, shift(500f, 300f))
    }

    @Test fun aPointUnderTheSheetScrollsJustEnoughToLiftIt() {
        // The pin's bottom is at 1500 + 50; it must be at 2000 - 600 - 20 = 1380.
        assertEquals(0f to 170f, shift(500f, 1500f))
    }

    @Test fun aPointOffASideScrollsItBackIn() {
        assertEquals(-30f to 0f, shift(-10f, 800f))
        assertEquals(40f to 0f, shift(1020f, 800f))
    }

    @Test fun aBlockTallerThanTheGapKeepsItsTopInView() {
        // Callout top at 0, pin bottom at 1400: both cannot fit in 120..1380, so the top wins.
        assertEquals(0f to -120f, shift(500f, 1000f, above = 1000f, below = 400f))
    }

    @Test fun onlyAShiftOfMoreThanAScreenIsFar() {
        assertEquals(false, FocusShift.far(1000f to -2000f, view))
        assertEquals(true, FocusShift.far(1001f to 0f, view))
        assertEquals(true, FocusShift.far(0f to -2001f, view))
    }

    @Test fun aBlockTooLowIsLiftedOnlyAsFarAsItsTopAllows() {
        // Pin bottom at 1700 wants a lift of 320, but the callout top at 200 can only rise by 80 to stay at 120.
        assertEquals(0f to 80f, shift(500f, 1100f, above = 900f, below = 600f))
    }

    @Test fun aFarPointIsCentredTogetherWithItsCallout() {
        // The visible area runs 100..1400 (centre 750, x 500). The block runs from y - 300 to y + 50, so its middle is y - 125.
        assertEquals(4500f to 5125f, FocusShift.centred(5000f, 6000f, 300f, 50f, view))
        assertEquals(0f to 0f, FocusShift.centred(500f, 875f, 300f, 50f, view))
    }
}
