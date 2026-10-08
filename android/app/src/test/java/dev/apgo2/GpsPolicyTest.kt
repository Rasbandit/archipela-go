package dev.apgo2

import dev.apgo2.presence.Decision
import dev.apgo2.presence.GpsMode
import dev.apgo2.presence.PresenceState
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

class GpsProvidersTest {
    @Test fun prefersFusedThenGpsAndNeverMixesProviders() {
        assertEquals(listOf("fused"), GpsPolicy.providers(setOf("fused", "gps", "network"), sdk = 34))
        assertEquals(listOf("gps"), GpsPolicy.providers(setOf("gps", "network"), sdk = 34))
        assertEquals(listOf("gps"), GpsPolicy.providers(setOf("fused", "gps", "network"), sdk = 30)) // fused provider needs Android 12
    }

    @Test fun networkIsTheLastResortAndNothingEnabledMeansNothing() {
        assertEquals(listOf("network"), GpsPolicy.providers(setOf("network"), sdk = 34))
        assertEquals(emptyList<String>(), GpsPolicy.providers(emptySet(), sdk = 34))
    }
}

class GpsDecisionTest {
    @Test fun aRateDecisionBecomesThatRate() {
        val d = Decision(PresenceState.InZone, GpsMode.Rate(5_000L, 0f), counting = true)
        assertEquals("rate", GpsPolicy.Rate(5_000L, 0f), GpsPolicy.forDecision(d, appVisible = false))
    }

    @Test fun offMeansNoLocationEvenWhenTheAppIsOnScreen() {
        val d = Decision(PresenceState.AtHome, GpsMode.Off, counting = false)
        assertEquals("off", null, GpsPolicy.forDecision(d, appVisible = true))
    }

    @Test fun stoppedUsesTheIdleRuleOnlyWhileTheAppIsVisible() {
        val d = Decision(PresenceState.Stopped, GpsMode.Off, counting = false)
        assertEquals("visible", GpsPolicy.forState(playing = false), GpsPolicy.forDecision(d, appVisible = true))
        assertEquals("hidden", null, GpsPolicy.forDecision(d, appVisible = false))
    }
}
