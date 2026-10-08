package dev.apgo2

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ThrottleTest {
    @Test fun firstCallIsAlwaysDue() = assertTrue(Throttle(10_000).due(5))

    @Test fun repeatsInsideTheGapAreSkippedAndOutsideAreDue() {
        val t = Throttle(10_000)
        assertTrue(t.due(1_000))
        assertFalse(t.due(5_000))
        assertFalse(t.due(10_999))
        assertTrue(t.due(11_000))
        assertFalse(t.due(12_000))
    }

    @Test fun clockGoingBackwardsResets() {
        val t = Throttle(10_000)
        assertTrue(t.due(50_000))
        assertTrue(t.due(1_000)) // simulator clock or a changed system time
    }

    @Test fun resetMakesTheNextCallDue() {
        val t = Throttle(10_000)
        t.due(1_000)
        t.reset()
        assertTrue(t.due(1_001))
    }
}
