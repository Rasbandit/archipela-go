package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RecapTimelineTest {
    @Test fun durationIsProportionalToGainAndClamped() {
        val s = RecapTimeline.slots(listOf(0.41f, 0.1f, 1.0f))
        assertEquals(1066L, s[0].durationMs) // 0.41 * 2600
        assertEquals(450L, s[1].durationMs) // min
        assertEquals(2400L, s[2].durationMs) // max
    }

    @Test fun barsRunOneAfterAnother() {
        val s = RecapTimeline.slots(listOf(0.5f, 0.5f))
        assertEquals(RecapTimeline.START_MS, s[0].startMs)
        assertEquals(s[0].startMs + s[0].durationMs + RecapTimeline.GAP_MS, s[1].startMs)
        assertEquals(s[1].startMs + s[1].durationMs, RecapTimeline.endMs(s))
    }

    @Test fun noBarsIsEmpty() {
        assertEquals(emptyList<RecapTimeline.Slot>(), RecapTimeline.slots(emptyList()))
        assertEquals(0L, RecapTimeline.endMs(emptyList()))
    }

    @Test fun softLandIsLinearThenEndsExactlyAtOne() {
        assertEquals(0f, RecapTimeline.softLand(0f), 1e-6f)
        assertEquals(1f, RecapTimeline.softLand(1f), 1e-6f)
        val v = 1f / (1f - RecapTimeline.LANDING / 2f)
        assertEquals(0.5f * v, RecapTimeline.softLand(0.5f), 1e-5f) // linear part
        val a = RecapTimeline.softLand(0.879f)
        val b = RecapTimeline.softLand(0.881f)
        assertTrue("continuous at the joint", kotlin.math.abs(b - a) < 0.01f)
        assertTrue("never overshoots", RecapTimeline.softLand(0.99f) <= 1f)
    }

    @Test fun progressIsZeroBeforeAndOneAfterTheSlot() {
        val slot = RecapTimeline.Slot(1000, 1000)
        assertEquals(0f, RecapTimeline.progress(0, slot), 1e-6f)
        assertEquals(1f, RecapTimeline.progress(5000, slot), 1e-6f)
    }

    @Test fun gainFractionIsShareOfBarAndClampedToMax() {
        assertEquals(0.5f, RecapTimeline.gainFraction(0.0, 50.0, 100.0), 1e-6f)
        assertEquals(0.25f, RecapTimeline.gainFraction(50.0, 75.0, 100.0), 1e-6f)
        assertEquals(0.5f, RecapTimeline.gainFraction(50.0, 500.0, 100.0), 1e-6f) // after clamped to max
        assertEquals(0f, RecapTimeline.gainFraction(200.0, 300.0, 100.0), 1e-6f) // already past max
        assertEquals(0f, RecapTimeline.gainFraction(0.0, 10.0, 0.0), 1e-6f) // no max
    }

    @Test fun fadeInStartsAtSixtyPercentOfTheSlot() {
        val slot = RecapTimeline.Slot(1000, 1000)
        assertEquals(0f, RecapTimeline.fadeIn(1500, slot), 1e-6f)
        assertEquals(0f, RecapTimeline.fadeIn(1600, slot), 1e-6f)
        assertEquals(1f, RecapTimeline.fadeIn(2000, slot), 1e-6f)
        assertEquals(1f, RecapTimeline.fadeIn(9000, slot), 1e-6f)
        assertTrue(RecapTimeline.fadeIn(1800, slot) in 0.01f..0.99f)
    }
}
