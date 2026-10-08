package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PresencePolicyTest {
    private fun s(playing: Boolean = true, home: Boolean? = false, car: Boolean? = false, zone: Zone = Zone.Inside) = Signals(playing, home, car, zone)

    @Test fun notPlayingIsStoppedAndNothingCounts() {
        val d = PresencePolicy.decide(s(playing = false))
        assertEquals(PresenceState.Stopped, d.state)
        assertEquals(false, d.counting)
    }

    @Test fun theCarBeatsEverythingElse() {
        val d = PresencePolicy.decide(s(car = true, home = true, zone = Zone.Inside))
        assertEquals(Decision(PresenceState.InCar, GpsMode.Off, counting = false), d)
    }

    @Test fun notPlayingBeatsCarAndHome() {
        assertEquals(Decision(PresenceState.Stopped, GpsMode.Off, counting = false), PresencePolicy.decide(s(playing = false, car = true, home = true)))
    }

    @Test fun theCoarseIntervalIsNinetySeconds() = assertEquals(90_000L, PresencePolicy.COARSE_MS)

    @Test fun homeWifiTurnsGpsOffAndStopsCounting() {
        assertEquals(Decision(PresenceState.AtHome, GpsMode.Off, counting = false), PresencePolicy.decide(s(home = true, zone = Zone.Far)))
    }

    @Test fun insideNearOrUnknownZoneIsPreciseAndCounts() {
        for (z in listOf(Zone.Inside, Zone.Near, Zone.Unknown)) {
            assertEquals("$z", Decision(PresenceState.InZone, GpsMode.Rate(5_000L, 0f), counting = true), PresencePolicy.decide(s(zone = z)))
        }
    }

    @Test fun farFromEveryZoneIsCoarseButStillCounts() {
        assertEquals(Decision(PresenceState.OutsideZones, GpsMode.Rate(PresencePolicy.COARSE_MS, 0f), counting = true), PresencePolicy.decide(s(zone = Zone.Far)))
    }

    @Test fun unknownSignalsAreTreatedAsNotPresent() {
        assertEquals(PresenceState.InZone, PresencePolicy.decide(s(home = null, car = null, zone = Zone.Inside)).state)
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

    @Test fun unknownIsAdoptedImmediatelyAndNeverHeldBack() {
        val d = Debouncer(45_000)
        d.feed(true, 0)
        assertNull(d.feed(null, 1_000))
    }
}
