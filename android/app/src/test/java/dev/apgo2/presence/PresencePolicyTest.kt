package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PresencePolicyTest {
    private fun signals(
        playing: Boolean = true,
        home: Boolean? = false,
        car: Boolean? = false,
        zone: Zone = Zone.Inside,
        checking: Boolean = false,
    ) = Signals(playing, home, car, zone, checking)

    @Test fun aHomeCheckLooksWithGpsButCountsNothing() {
        // The core is telling whether a walk or a Wi-Fi drop left the home circle: GPS on, still home, nothing counts.
        assertEquals(
            Decision(PresenceState.AtHome, GpsMode.Check, counting = false),
            PresencePolicy.decide(signals(home = true, checking = true, zone = Zone.Far)),
        )
        assertEquals(
            "the car still wins",
            PresenceState.InCar,
            PresencePolicy.decide(signals(home = true, checking = true, car = true)).state,
        )
    }

    @Test fun notPlayingIsStoppedAndNothingCounts() {
        val d = PresencePolicy.decide(signals(playing = false))
        assertEquals(PresenceState.Stopped, d.state)
        assertEquals(false, d.counting)
    }

    @Test fun arrivingHomeIsOnlyTheChangeIntoAtHome() {
        assertTrue(PresencePolicy.arrivedAtCamp(PresenceState.InZone, PresenceState.AtHome))
        assertTrue(PresencePolicy.arrivedAtCamp(PresenceState.OutsideZones, PresenceState.AtHome))
        assertTrue("from the car to home", PresencePolicy.arrivedAtCamp(PresenceState.InCar, PresenceState.AtHome))
        assertTrue("opening a game at home", PresencePolicy.arrivedAtCamp(PresenceState.Stopped, PresenceState.AtHome))
        assertFalse("still home", PresencePolicy.arrivedAtCamp(PresenceState.AtHome, PresenceState.AtHome))
        assertFalse("leaving home", PresencePolicy.arrivedAtCamp(PresenceState.AtHome, PresenceState.InZone))
        assertFalse(PresencePolicy.arrivedAtCamp(PresenceState.InZone, PresenceState.InCar))
    }

    @Test fun aHomeCheckIsNotArrivingHome() {
        // Presence (re)started away from home with a home pin and no home Wi-Fi: the core checks with GPS, shown as AtHome. That is
        // not arriving home, so forager quests must not bank mid-walk.
        assertFalse(PresencePolicy.arrivedAtCamp(PresenceState.Stopped, PresenceState.AtHome, checking = true))
        assertFalse(PresencePolicy.arrivedAtCamp(PresenceState.InZone, PresenceState.AtHome, checking = true))
    }

    @Test fun aHomeCheckThatEndsAtHomeIsArrivingHome() {
        // A cold start at home with no home Wi-Fi: Stopped -> AtHome while checking, then the check says "home" with the state
        // unchanged. That is the arrival, so the stale outing closes.
        assertTrue(PresencePolicy.arrivedAtCamp(PresenceState.AtHome, PresenceState.AtHome, wasChecking = true, checking = false))
        assertFalse(
            "still checking",
            PresencePolicy.arrivedAtCamp(PresenceState.AtHome, PresenceState.AtHome, wasChecking = true, checking = true),
        )
        assertFalse(
            "a check that finds a walk leaves home",
            PresencePolicy.arrivedAtCamp(PresenceState.AtHome, PresenceState.InZone, wasChecking = true),
        )
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

    @Test fun farAwayStandingStillProducesNoFixes() {
        // Dwell and time away finish on a scheduled tick, so far from every zone GPS only needs to report 50 m moves. In a zone
        // there is no distance filter: the core's stationary hold and gap bridge need a steady stream (GpsPolicy.inZone).
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
                Decision(PresenceState.InZone, GpsMode.Precise, counting = true),
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

    @Test fun leavingTrueCanSettleFasterThanArriving() {
        // The car: getting out (Bluetooth gone) counts after 5 s, getting in still after 45 s.
        val d = Debouncer(45_000, leaveTrueMs = 5_000)
        d.feed(true, 0)
        assertEquals("just disconnected", true, d.feed(false, 1_000))
        assertEquals("one ms short of 5 s", true, d.feed(false, 5_999))
        assertEquals("held 5 s", false, d.feed(false, 6_000))
        assertEquals("reconnected", false, d.feed(true, 7_000))
        assertEquals("arriving still waits 45 s", false, d.feed(true, 51_999))
        assertEquals("held 45 s", true, d.feed(true, 52_000))
    }

    @Test fun theCarLetsYouOutAfterFiveSecondsAndInAfterFortyFive() {
        val d = PresencePolicy.carDebouncer()
        d.feed(true, 0)
        d.feed(false, 1_000)
        assertEquals("out after 5 s", false, d.feed(false, 6_000))
        d.feed(true, 7_000)
        assertEquals(45_000L, d.settlesInMs(7_000))
    }

    @Test fun aDropShorterThanTheLeaveHoldNeverSettles() {
        val d = Debouncer(45_000, leaveTrueMs = 5_000)
        d.feed(true, 0)
        d.feed(false, 1_000)
        assertEquals("back within 5 s", true, d.feed(true, 4_000))
        assertNull("nothing pending", d.settlesInMs(4_000))
    }

    @Test fun leavingTrueForUnknownUsesTheLeaveHold() {
        val d = Debouncer(45_000, leaveTrueMs = 5_000)
        d.feed(true, 0)
        d.feed(null, 1_000)
        assertEquals("settles in the leave hold", 5_000L, d.settlesInMs(1_000))
        assertNull(d.feed(null, 6_000))
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
