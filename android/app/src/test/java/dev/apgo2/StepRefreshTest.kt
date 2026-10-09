package dev.apgo2

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StepRefreshTest {
    @Test fun refreshesOnTheFirstReadingThenOnlyWhenStepsMovedEnoughToShow() {
        val r = StepRefresh(minSteps = 50)
        assertTrue("first reading", r.due(1_000))
        assertFalse("49 more: nothing visible changed", r.due(1_049))
        assertTrue("50 since the last refresh", r.due(1_050))
        assertFalse("the same reading again", r.due(1_050))
    }

    @Test fun aCounterResetCountsAsAChange() {
        val r = StepRefresh(minSteps = 50)
        r.due(10_000)
        assertTrue("phone rebooted: the counter starts over", r.due(5))
    }
}
