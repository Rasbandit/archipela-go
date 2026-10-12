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

    @Test fun aWholeWaitWithNoDataMeansTheConnectionDied() {
        // The Archipelago server pings every 20 s: 60 s of silence is a socket that died without telling (network switch, NAT).
        assertTrue(ApWait.dead(slept = true, woke = false))
        assertFalse("the server sent something", ApWait.dead(slept = true, woke = true))
        assertFalse("did not wait (connecting, or busy)", ApWait.dead(slept = false, woke = false))
    }

    @Test fun aDroppedConnectionReconnectsButARefusedFirstConnectDoesNot() {
        // Archipelago hard requirement: reconnect if the connection is lost while playing.
        assertTrue(ApWait.reconnects(status = "disconnected: connection reset", everConnected = true))
        assertFalse(
            "never connected (wrong slot, password or address): the player fixes it",
            ApWait.reconnects("disconnected: refused", false),
        )
        assertFalse(ApWait.reconnects(status = "connected", everConnected = true))
        assertFalse(ApWait.reconnects(status = "connecting", everConnected = true))
    }

    @Test fun onOpeningTheAppAStaleOrDroppedSessionReconnectsAtOnce() {
        // Asleep (Doze, a frozen process) the wait's timer cannot run; elapsed real time still tells how long the server was silent.
        assertTrue("connected but silent past the 20 s pings", ApWait.stale("connected", everConnected = true, silentMs = 46_000))
        assertFalse(ApWait.stale("connected", everConnected = true, silentMs = 30_000))
        assertTrue("dropped", ApWait.stale("disconnected: reset", everConnected = true, silentMs = 0))
        assertFalse("still connecting", ApWait.stale("connecting", everConnected = true, silentMs = 90_000))
    }
}
