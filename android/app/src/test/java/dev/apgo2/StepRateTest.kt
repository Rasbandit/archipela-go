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

    @Test fun aZoneGetsTheCadenceEveryTwoSecondsOnScreenAndFiveOff() {
        assertEquals(2_000_000, StepRate.batchUs(PresenceState.InZone, inZone = true, onScreen = true))
        // The same 5 s as the compass off screen (loc_scenarios screen_off_batching_...): the bridge loses nothing.
        assertEquals(5_000_000, StepRate.batchUs(PresenceState.InZone, inZone = true, onScreen = false))
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

    @Test fun theStepCounterWakesThePhoneOnlyWhereItsReadingsMustArriveOnTime() {
        // Off screen in a zone (the gap bridge) and while leaving (the leave mark): a wake-up sensor delivers each batch on time.
        assertEquals(true, StepRate.wakeUp(PresenceState.InZone, inZone = true, leaving = false, onScreen = false))
        assertEquals(true, StepRate.wakeUp(PresenceState.AtHome, inZone = false, leaving = true, onScreen = false))
        assertEquals(
            "on screen: awake anyway",
            false,
            StepRate.wakeUp(PresenceState.InZone, inZone = true, leaving = false, onScreen = true),
        )
        assertEquals("outside zones: whenever the phone wakes", false, StepRate.wakeUp(PresenceState.OutsideZones, false, false, false))
    }

    @Test fun leavingIsOnlyThePlaceThePlayerIsAtGoingAway() {
        assertEquals(true, StepRate.leaving(PresenceState.AtHome, homePending = true, carPending = false))
        assertEquals(true, StepRate.leaving(PresenceState.InCar, homePending = false, carPending = true))
        assertEquals("a car connecting at home is arriving", false, StepRate.leaving(PresenceState.AtHome, false, carPending = true))
        assertEquals("home Wi-Fi seen from the car", false, StepRate.leaving(PresenceState.InCar, homePending = true, carPending = false))
        assertEquals("walking up to home: arriving", false, StepRate.leaving(PresenceState.InZone, homePending = true, carPending = true))
    }

    @Test fun outsideTheZonesStepsCountEveryTenSeconds() {
        assertEquals(10_000_000, StepRate.batchUs(PresenceState.OutsideZones, inZone = false))
        assertEquals("in a zone without location: no cadence wanted", 10_000_000, StepRate.batchUs(PresenceState.InZone, inZone = false))
    }
}
