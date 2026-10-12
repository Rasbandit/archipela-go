package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TrapAlertTest {
    // #160: one long, harsh buzz, hard to miss in a pocket: several strong pulses, over two seconds on in all.
    @Test fun theBuzzIsLongAndHarsh() {
        val p = TrapAlert.PATTERN
        assertEquals("starts at once", 0L, p[0])
        val on = p.filterIndexed { i, _ -> i % 2 == 1 }
        assertTrue("several pulses", on.size >= 3)
        assertTrue("over two seconds on", on.sum() >= 2000L)
    }
}
