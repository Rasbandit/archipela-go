package dev.apgo2

import dev.apgo2.presence.PresenceState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StepRateTest {
    @Test fun noGameOpenNeedsNoStepCounter() {
        // Steps count only with a game open, and opening one resets the baseline.
        assertNull(StepRate.batchUs(PresenceState.Stopped, inZone = false))
    }

    @Test fun aZoneGetsTheCadenceEveryTwoSeconds() {
        assertEquals(2_000_000, StepRate.batchUs(PresenceState.InZone, inZone = true))
    }

    @Test fun atHomeOrInTheCarTheStepCounterIsOff() {
        // Nothing counts there, and the core takes the first reading after counting resumes as the new baseline.
        assertNull(StepRate.batchUs(PresenceState.AtHome, inZone = false))
        assertNull(StepRate.batchUs(PresenceState.InCar, inZone = false))
    }

    @Test fun leavingHomeOrTheCarStartsTheCounterBeforeItIsConfirmed() {
        // Wi-Fi or Bluetooth dropped, the 45 s debounce still runs: the first reading is the core's leave mark (backfill).
        assertEquals(10_000_000, StepRate.batchUs(PresenceState.AtHome, inZone = false, leaving = true))
        assertEquals(10_000_000, StepRate.batchUs(PresenceState.InCar, inZone = false, leaving = true))
        assertNull("no game: nothing to backfill", StepRate.batchUs(PresenceState.Stopped, inZone = false, leaving = true))
    }

    @Test fun outsideTheZonesStepsCountEveryTenSeconds() {
        assertEquals(10_000_000, StepRate.batchUs(PresenceState.OutsideZones, inZone = false))
        assertEquals("in a zone without location: no cadence wanted", 10_000_000, StepRate.batchUs(PresenceState.InZone, inZone = false))
    }
}
