package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PresenceSignalsTest {
    @Test fun ssidQuotesAreStrippedAndUnknownNamesBecomeNull() {
        assertEquals("HomeNet", PresenceSignals.cleanSsid("\"HomeNet\""))
        assertEquals("HomeNet", PresenceSignals.cleanSsid("HomeNet"))
        assertNull(PresenceSignals.cleanSsid("<unknown ssid>"))
        assertNull(PresenceSignals.cleanSsid("\"<unknown ssid>\""))
        assertNull(PresenceSignals.cleanSsid(""))
        assertNull(PresenceSignals.cleanSsid(null))
    }

    @Test fun aSavedNetworkMatchesByBssidOrSsid() {
        val saved = listOf(HomeNetwork("HomeNet", "aa:bb:cc:dd:ee:01"))
        assertEquals(true, PresenceSignals.isHome(WifiId("HomeNet", null), saved))
        assertEquals("same router, new name, case-insensitive", true, PresenceSignals.isHome(WifiId("Renamed", "AA:BB:CC:DD:EE:01"), saved))
        assertEquals(false, PresenceSignals.isHome(WifiId("CafeWifi", "11:22:33:44:55:66"), saved))
    }

    @Test fun aBssidMismatchWithTheSameSsidStillCountsBecauseHomesHaveSeveralAccessPoints() {
        val saved = listOf(HomeNetwork("HomeNet", "aa:bb:cc:dd:ee:01"))
        assertEquals(true, PresenceSignals.isHome(WifiId("HomeNet", "aa:bb:cc:dd:ee:02"), saved))
    }

    @Test fun noConnectionOrNothingSavedIsUnknownNotFalse() {
        assertNull(PresenceSignals.isHome(null, listOf(HomeNetwork("HomeNet", null))))
        assertNull(PresenceSignals.isHome(WifiId(null, null), listOf(HomeNetwork("HomeNet", null))))
        assertNull(PresenceSignals.isHome(WifiId("HomeNet", null), emptyList()))
    }

    @Test fun theCarIsConnectedWhenATaggedDeviceIsConnected() {
        val saved = listOf(CarDevice("Subaru", "AA:AA:AA:AA:AA:01"))
        assertEquals(true, PresenceSignals.carConnected(setOf("aa:aa:aa:aa:aa:01", "BB:BB:BB:BB:BB:02"), saved))
        assertEquals(false, PresenceSignals.carConnected(setOf("BB:BB:BB:BB:BB:02"), saved))
        assertEquals(false, PresenceSignals.carConnected(emptySet(), saved))
    }

    @Test fun noTaggedCarOrAnUnreadableListIsUnknown() {
        assertNull(PresenceSignals.carConnected(setOf("AA"), emptyList()))
        assertNull(PresenceSignals.carConnected(null, listOf(CarDevice("Subaru", "AA"))))
    }
}
