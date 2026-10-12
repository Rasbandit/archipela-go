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

    @Test fun atHomeOrInTheCarStepsOnlyKeepTheBaseline() {
        // Not off: the baseline from just before leaving keeps steps taken indoors from being credited.
        assertEquals(60_000_000, StepRate.batchUs(PresenceState.AtHome, inZone = false))
        assertEquals(60_000_000, StepRate.batchUs(PresenceState.InCar, inZone = false))
    }

    @Test fun outsideTheZonesStepsCountEveryTenSeconds() {
        assertEquals(10_000_000, StepRate.batchUs(PresenceState.OutsideZones, inZone = false))
        assertEquals("in a zone without location: no cadence wanted", 10_000_000, StepRate.batchUs(PresenceState.InZone, inZone = false))
    }
}
