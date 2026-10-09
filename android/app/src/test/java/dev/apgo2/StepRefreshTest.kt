package dev.apgo2

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StepRefreshTest {
    @Test fun refreshesOnTheFirstReadingThenOnlyWhenStepsMovedEnoughToShow() {
        val r = StepRefresh(minSteps = 50)
        assertTrue("first reading", r.due(1_000, "g"))
        assertFalse("49 more: nothing visible changed", r.due(1_049, "g"))
        assertTrue("50 since the last refresh", r.due(1_050, "g"))
        assertFalse("the same reading again", r.due(1_050, "g"))
    }

    @Test fun aCounterResetCountsAsAChange() {
        val r = StepRefresh(minSteps = 50)
        r.due(10_000, "g")
        assertTrue("phone rebooted: the counter starts over", r.due(5, "g"))
    }

    @Test fun anotherGameStartsOver() {
        val r = StepRefresh(minSteps = 50)
        r.due(10_020, "a")
        assertTrue("the first reading of a newly opened game always refreshes", r.due(10_030, "b"))
    }
}
