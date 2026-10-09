package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class MapPaddingTest {
    private val eps = 1e-6f

    @Test fun withNoOverlaysTheCentreIsTheMiddle() {
        assertEquals(500f, MapPadding.centerY(1000f, 0f, 0f), eps)
    }

    @Test fun theCentreIsTheMiddleOfWhatTheOverlaysLeave() {
        assertEquals(400f, MapPadding.centerY(1000f, 100f, 300f), eps)
    }

    @Test fun overlaysCoveringEverythingPutTheCentreUnderTheTopOne() {
        assertEquals(700f, MapPadding.centerY(1000f, 700f, 600f), eps)
    }
}
