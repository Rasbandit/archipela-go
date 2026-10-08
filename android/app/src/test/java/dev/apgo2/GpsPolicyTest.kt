package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class GpsPolicyTest {
    @Test fun playingPingsEveryFiveSecondsEvenWhenStanding() {
        // Dwell and Away quests accrue time from fixes: a distance filter would starve them while you stand still.
        val r = GpsPolicy.forState(playing = true)
        assertEquals(5_000L, r.intervalMs)
        assertEquals(0f, r.minDistanceM)
    }

    @Test fun idlePingsLessOften() {
        val r = GpsPolicy.forState(playing = false)
        assertEquals(15_000L, r.intervalMs)
        assertEquals(20f, r.minDistanceM)
    }

    @Test fun playingIsAlwaysFinerThanIdle() {
        val p = GpsPolicy.forState(true)
        val i = GpsPolicy.forState(false)
        assertTrue(p.intervalMs < i.intervalMs && p.minDistanceM <= i.minDistanceM)
    }

    @Test fun playingStaysUnderTheCoreGapLimit() {
        // core verify.rs drops gaps over 5 minutes (Away quests): the cadence must stay far below that.
        assertTrue(GpsPolicy.forState(true).intervalMs * 10 < 5 * 60_000L)
    }
}
