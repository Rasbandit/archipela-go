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

    @Test fun thePanelFollowsTheFingerWithinItsTwoEnds() {
        assertEquals(0.75f, PaneMode.openAfterMove(open = 1f, dyPx = 100f, bodyPx = 400f), 1e-6f)
        assertEquals(0.5f, PaneMode.openAfterMove(open = 0.25f, dyPx = -100f, bodyPx = 400f), 1e-6f)
        assertEquals("never past hidden", 0f, PaneMode.openAfterMove(open = 0.1f, dyPx = 400f, bodyPx = 400f), 0f)
        assertEquals("never past shown", 1f, PaneMode.openAfterMove(open = 0.9f, dyPx = -400f, bodyPx = 400f), 0f)
        assertEquals("no size yet", 0.6f, PaneMode.openAfterMove(open = 0.6f, dyPx = 50f, bodyPx = 0f), 0f)
    }

    @Test fun draggingTheWayItAlreadyIsKeepsTheMode() {
        assertEquals(true, PaneMode.afterDrag(shown = true, dragPx = -500f, thresholdPx = threshold))
        assertEquals(false, PaneMode.afterDrag(shown = false, dragPx = 500f, thresholdPx = threshold))
    }
}
