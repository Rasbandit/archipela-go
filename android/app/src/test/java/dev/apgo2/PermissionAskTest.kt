package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PermissionAskTest {
    @Test fun grantedNeedsNoButton() {
        assertEquals(PermissionAsk.Granted, permissionAsk(granted = true, deniedBefore = false, showRationale = false))
    }

    @Test fun grantedWinsOverAnEarlierDenial() {
        // Allowed on the settings page after a permanent denial.
        assertEquals(PermissionAsk.Granted, permissionAsk(granted = true, deniedBefore = true, showRationale = false))
    }

    @Test fun neverAskedRequests() {
        // Before the first request the rationale flag is false too: that must not read as permanently denied.
        assertEquals(PermissionAsk.Request, permissionAsk(granted = false, deniedBefore = false, showRationale = false))
    }

    @Test fun deniedOnceCanStillBeRequested() {
        assertEquals(PermissionAsk.Request, permissionAsk(granted = false, deniedBefore = true, showRationale = true))
    }

    @Test fun deniedWithoutRationaleOpensSettings() {
        assertEquals(PermissionAsk.OpenSettings, permissionAsk(granted = false, deniedBefore = true, showRationale = false))
    }

    @Test fun rationaleWithoutRecordedDenialRequests() {
        // Denied before the flag was recorded (prefs cleared): the system still shows the dialog.
        assertEquals(PermissionAsk.Request, permissionAsk(granted = false, deniedBefore = false, showRationale = true))
    }

    @Test fun grantClearsTheDenial() {
        // Also covers Android auto-resetting an unused permission later: the next ask starts fresh.
        assertFalse(deniedAfterAnswer(granted = true, rationaleNow = false, deniedBefore = true))
    }

    @Test fun firstDenialIsRecorded() {
        assertTrue(deniedAfterAnswer(granted = false, rationaleNow = true, deniedBefore = false))
    }

    @Test fun dismissedDialogRecordsNothing() {
        // Tap outside or Back on Android 11+: denied, and the rationale flag stays false.
        assertFalse(deniedAfterAnswer(granted = false, rationaleNow = false, deniedBefore = false))
    }

    @Test fun secondDenialKeepsTheRecord() {
        assertTrue(deniedAfterAnswer(granted = false, rationaleNow = false, deniedBefore = true))
    }

    @Test fun dismissedDialogDoesNotOpenSettings() {
        val denied = deniedAfterAnswer(granted = false, rationaleNow = false, deniedBefore = false)
        assertEquals(PermissionAsk.Request, permissionAsk(granted = false, deniedBefore = denied, showRationale = false))
    }
}
