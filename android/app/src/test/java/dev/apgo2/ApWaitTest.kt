package dev.apgo2

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ApWaitTest {
    @Test fun aQuietConnectedSessionSleepsUntilTheServerSendsSomething() {
        assertTrue(ApWait.sleeps(status = "connected", active = false))
    }

    @Test fun otherwiseTheCoresBackOffTimes() {
        assertFalse("the server just spoke: drain again soon", ApWait.sleeps(status = "connected", active = true))
        assertFalse("no socket yet", ApWait.sleeps(status = "connecting", active = false))
        assertFalse(ApWait.sleeps(status = "disconnected: refused", active = false))
    }
}
