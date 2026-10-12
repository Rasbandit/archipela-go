package dev.apgo2

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenGateTest {
    @Test fun startsOffScreenUntilShown() = assertFalse(ScreenGate().draw())

    @Test fun onScreenDrawsAndOwesNothing() {
        val g = ScreenGate()
        g.show()
        assertTrue(g.draw())
        g.hide()
        assertFalse(g.show())
    }

    @Test fun offScreenSkipsAndOwesOneRedraw() {
        val g = ScreenGate()
        g.show()
        g.hide()
        assertFalse(g.draw())
        assertFalse(g.draw())
        assertTrue(g.show())
        assertFalse(g.show())
    }

    @Test fun nothingOwedWhenNothingWasSkipped() {
        val g = ScreenGate()
        assertFalse(g.show())
    }

    @Test fun aSkipBeforeTheFirstShowIsOwed() {
        val g = ScreenGate()
        assertFalse(g.draw()) // the process loads the game before the activity starts
        assertTrue(g.show())
    }
}
