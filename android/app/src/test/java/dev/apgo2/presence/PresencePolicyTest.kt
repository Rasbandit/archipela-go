package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PresencePolicyTest {
    private fun signals(
        playing: Boolean = true,
        home: Boolean? = false,
        car: Boolean? = false,
        zone: Zone = Zone.Inside,
    ) = Signals(playing, home, car, zone)

    @Test fun notPlayingIsStoppedAndNothingCounts() {
        val d = PresencePolicy.decide(signals(playing = false))
        assertEquals(PresenceState.Stopped, d.state)
        assertEquals(false, d.counting)
    }

    @Test fun theCarBeatsEverythingElse() {
        val d = PresencePolicy.decide(signals(car = true, home = true, zone = Zone.Inside))
        assertEquals(Decision(PresenceState.InCar, GpsMode.Off, counting = false), d)
    }

    @Test fun notPlayingBeatsCarAndHome() {
        assertEquals(
            Decision(PresenceState.Stopped, GpsMode.Off, counting = false),
            PresencePolicy.decide(signals(playing = false, car = true, home = true)),
        )
    }

    @Test fun theCoarseIntervalIsNinetySeconds() = assertEquals(90_000L, PresencePolicy.COARSE_MS)

    @Test fun standingStillProducesNoFixes() {
        // Dwell and time away finish on a scheduled tick, so GPS only needs to report movement: 10 m in a zone, 50 m far away.
        assertEquals(10f, PresencePolicy.ZONE_MOVE_M)
        assertEquals(50f, PresencePolicy.FAR_MOVE_M)
    }

    @Test fun homeWifiTurnsGpsOffAndStopsCounting() {
        assertEquals(
            Decision(PresenceState.AtHome, GpsMode.Off, counting = false),
            PresencePolicy.decide(signals(home = true, zone = Zone.Far)),
        )
    }

    @Test fun insideNearOrUnknownZoneIsPreciseAndCounts() {
        for (z in listOf(Zone.Inside, Zone.Near, Zone.Unknown)) {
            assertEquals(
                "$z",
                Decision(PresenceState.InZone, GpsMode.Rate(5_000L, PresencePolicy.ZONE_MOVE_M), counting = true),
                PresencePolicy.decide(signals(zone = z)),
            )
        }
    }

    @Test fun farFromEveryZoneIsCoarseButStillCounts() {
        assertEquals(
            Decision(PresenceState.OutsideZones, GpsMode.Rate(PresencePolicy.COARSE_MS, PresencePolicy.FAR_MOVE_M), counting = true),
            PresencePolicy.decide(signals(zone = Zone.Far)),
        )
    }

    @Test fun unknownSignalsAreTreatedAsNotPresent() {
        assertEquals(PresenceState.InZone, PresencePolicy.decide(signals(home = null, car = null, zone = Zone.Inside)).state)
    }
}

class DebouncerTest {
    @Test fun theFirstValueIsAdoptedAtOnce() = assertEquals(true, Debouncer(45_000).feed(true, 0))

    @Test fun aChangeOnlyTakesEffectAfterItHasHeldLongEnough() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        assertEquals("just changed", false, d.feed(true, 1_000))
        assertEquals("still inside the hold", false, d.feed(true, 30_000))
        assertEquals("held for 45 s", true, d.feed(true, 46_001))
    }

    @Test fun flappingNeverSettles() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        var t = 1_000L
        repeat(20) {
            assertEquals("flap $it at $t", false, d.feed(it % 2 == 0, t))
            t += 10_000
        }
        // The last flap fed false at t - 10_000; now hold true until it settles.
        assertEquals("sustained true starts", false, d.feed(true, t))
        assertEquals("sustained true settles", true, d.feed(true, t + 45_000))
    }

    @Test fun theHoldBoundaryIsInclusive() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        d.feed(true, 1_000)
        assertEquals("just under holdMs", false, d.feed(true, 45_999))
        assertEquals("exactly holdMs", true, d.feed(true, 46_000))
    }

    @Test fun aMidHoldRevertRestartsTheTimer() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        d.feed(true, 1_000)
        d.feed(false, 20_000)
        d.feed(true, 30_000)
        assertEquals("timer restarted at 30 s", false, d.feed(true, 50_000))
        assertEquals("held 45 s since 30 s", true, d.feed(true, 75_000))
    }

    @Test fun unknownIsHeldLikeAnyOtherChange() {
        val d = Debouncer(45_000)
        d.feed(true, 0)
        assertEquals("null just fed", true, d.feed(null, 1_000))
        assertEquals("one ms short of holdMs", true, d.feed(null, 45_999))
        assertNull("exactly holdMs", d.feed(null, 46_000))
    }

    @Test fun leavingForUnknownIsAdoptedAfterTheHoldFromTheFirstNull() {
        val d = Debouncer(45_000)
        d.feed(true, 0)
        d.feed(null, 5_000)
        assertEquals("44_999 ms after the null", true, d.feed(null, 49_999))
        assertNull("45_000 ms after the null", d.feed(null, 50_000))
    }

    @Test fun edgeOfRangeFlappingBetweenTrueAndNullNeverFlips() {
        val d = Debouncer(45_000)
        assertEquals(true, d.feed(true, 0))
        var flips = 0
        var last: Boolean? = true
        var t = 10_000L
        repeat(40) {
            val v = d.feed(if (it % 2 == 0) null else true, t)
            if (v != last) flips++
            last = v
            t += 10_000
        }
        assertEquals("stable value changes", 0, flips)
        assertEquals(true, last)
    }

    @Test fun seededTrueThenNullThenTrueWithinTheHoldStaysTrue() {
        val d = Debouncer(45_000)
        d.seed(true)
        assertEquals(true, d.feed(null, 1_000))
        assertEquals(true, d.feed(true, 20_000))
        assertEquals("long after the blip", true, d.feed(true, 100_000))
    }

    @Test fun pendingIsTrueWhileAChangeIsBeingHeld() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        assertEquals("steady", false, d.pending)
        d.feed(true, 1_000)
        assertEquals("holding", true, d.pending)
        d.feed(true, 46_000)
        assertEquals("settled", false, d.pending)
    }

    @Test fun seedAfterHistoryRestartsFromTheSeededValue() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        d.seed(true)
        assertEquals("seeded true is stable at once", true, d.feed(true, 1_000))
    }

    @Test fun aSeededValueIsHeldAgainstAChange() {
        val d = Debouncer(45_000)
        d.seed(true)
        assertEquals("false is only a change from the seeded true", true, d.feed(false, 0))
        assertEquals("until held long enough", false, d.feed(false, 45_000))
    }

    @Test fun seedingUnknownMeansNoHistoryAndTheNextValueIsAdoptedAtOnce() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        d.seed(null)
        assertNull("still unknown", d.feed(null, 500))
        assertEquals("first real value", true, d.feed(true, 1_000))
        assertEquals("then debounced as usual", true, d.feed(false, 2_000))
    }

    @Test fun pendingIsFalseRightAfterAnySeed() {
        val d = Debouncer(45_000)
        d.feed(true, 0)
        d.feed(false, 1_000)
        assertEquals("holding", true, d.pending)
        d.seed(false)
        assertEquals(false, d.pending)
        d.seed(null)
        assertEquals(false, d.pending)
    }

    @Test fun aPendingChangeSaysExactlyWhenItWillSettle() {
        val d = Debouncer(holdMs = 45_000)
        d.feed(false, 0)
        assertEquals("nothing pending", null, d.settlesInMs(1_000))
        d.feed(true, 10_000)
        assertEquals(45_000L, d.settlesInMs(10_000))
        assertEquals(15_000L, d.settlesInMs(40_000))
        assertEquals("overdue is now", 0L, d.settlesInMs(60_000))
        d.feed(true, 55_000)
        assertEquals("settled", null, d.settlesInMs(55_000))
    }
}
