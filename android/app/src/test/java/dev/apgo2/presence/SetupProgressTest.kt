package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SetupProgressTest {
    private fun p(
        home: Boolean = false,
        wifi: Int = 0,
        car: Int = 0,
        done: Boolean = false,
    ) = SetupProgress(home, wifi, car, done)

    @Test fun emptyStartsAtHomeAndNeedsAttention() {
        assertEquals(SetupStep.Home, p().nextStep())
        assertTrue(p().needsAttention())
    }

    @Test fun homeOnlyPointsAtWifi() {
        assertEquals(SetupStep.Wifi, p(home = true).nextStep())
        assertTrue(p(home = true).missingWifi)
    }

    @Test fun homeAndWifiPointsAtCar() {
        assertEquals(SetupStep.Car, p(home = true, wifi = 1).nextStep())
    }

    @Test fun everythingDoneHasNoNextStepAndNoNag() {
        val all = p(home = true, wifi = 2, car = 1, done = true)
        assertNull(all.nextStep())
        assertFalse(all.needsAttention())
    }

    @Test fun skippedWifiStillNagsAfterSetupIsDone() {
        val skipped = p(home = true, wifi = 0, car = 0, done = true)
        assertTrue("home set but no Wi-Fi", skipped.needsAttention())
    }

    @Test fun skippedCarDoesNotNag() {
        assertFalse(p(home = true, wifi = 1, car = 0, done = true).needsAttention())
    }

    @Test fun notDoneNagsEvenWhenComplete() {
        assertTrue("an upgrading user who never saw the wizard", p(home = true, wifi = 1, car = 1, done = false).needsAttention())
    }
}
