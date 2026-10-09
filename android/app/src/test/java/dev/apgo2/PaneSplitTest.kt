package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class PaneSplitTest {
    private val eps = 1e-6f

    @Test fun draggingDownGivesTheMapMore() {
        assertEquals(0.65f, PaneSplit.dragged(0.55f, 100f, 1000f, 0.9f), eps)
    }

    @Test fun draggingUpGivesThePanelMore() {
        assertEquals(0.45f, PaneSplit.dragged(0.55f, -100f, 1000f, 0.9f), eps)
    }

    @Test fun theMapNeverShrinksBelowItsMinimumOrPastThePeek() {
        assertEquals(PaneSplit.MIN_MAP, PaneSplit.dragged(0.55f, -5000f, 1000f, 0.9f), eps)
        assertEquals(0.9f, PaneSplit.dragged(0.55f, 5000f, 1000f, 0.9f), eps)
    }

    @Test fun noSizeYetMeansNoChange() {
        assertEquals(0.55f, PaneSplit.dragged(0.55f, 100f, 0f, 0.9f), eps)
    }

    @Test fun thePeekKeepsTheHandleAndAFirstLineInView() {
        assertEquals(0.92f, PaneSplit.maxMap(1000f, 80f), eps)
        assertEquals("a tiny screen still lets the map grow", PaneSplit.MIN_MAP, PaneSplit.maxMap(100f, 80f), eps)
        assertEquals("no size yet", PaneSplit.DEFAULT_MAP, PaneSplit.maxMap(0f, 80f), eps)
    }

    @Test fun aTapCollapsesThePanelOrBringsItBack() {
        assertEquals(0.92f, PaneSplit.toggled(0.55f, 0.92f), eps)
        assertEquals(0.92f, PaneSplit.toggled(0.4f, 0.92f), eps)
        assertEquals(PaneSplit.DEFAULT_MAP, PaneSplit.toggled(0.92f, 0.92f), eps)
        assertEquals("near enough the peek counts as collapsed", PaneSplit.DEFAULT_MAP, PaneSplit.toggled(0.915f, 0.92f), eps)
    }
}
