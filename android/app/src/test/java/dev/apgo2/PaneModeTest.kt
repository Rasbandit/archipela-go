package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class PaneModeTest {
    private val threshold = 40f

    @Test fun aPullDownPastTheThresholdHidesThePanel() {
        assertEquals(false, PaneMode.afterDrag(shown = true, dragPx = 41f, thresholdPx = threshold))
    }

    @Test fun aPushUpPastTheThresholdShowsIt() {
        assertEquals(true, PaneMode.afterDrag(shown = false, dragPx = -41f, thresholdPx = threshold))
    }

    @Test fun aShortDragKeepsTheMode() {
        assertEquals(true, PaneMode.afterDrag(shown = true, dragPx = 40f, thresholdPx = threshold))
        assertEquals(false, PaneMode.afterDrag(shown = false, dragPx = -40f, thresholdPx = threshold))
    }

    @Test fun draggingTheWayItAlreadyIsKeepsTheMode() {
        assertEquals(true, PaneMode.afterDrag(shown = true, dragPx = -500f, thresholdPx = threshold))
        assertEquals(false, PaneMode.afterDrag(shown = false, dragPx = 500f, thresholdPx = threshold))
    }
}
