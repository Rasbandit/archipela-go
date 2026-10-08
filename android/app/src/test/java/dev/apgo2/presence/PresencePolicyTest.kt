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
        repeat(20) { d.feed(it % 2 == 0, t); t += 10_000 }
        assertEquals(false, d.feed(false, t))
    }

    @Test fun unknownIsAdoptedImmediatelyAndNeverHeldBack() {
        val d = Debouncer(45_000)
        d.feed(true, 0)
        assertNull(d.feed(null, 1_000))
    }
}
