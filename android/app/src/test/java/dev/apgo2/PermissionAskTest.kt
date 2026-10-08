package dev.apgo2

import org.junit.Assert.assertEquals
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
}
