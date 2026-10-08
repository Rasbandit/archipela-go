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

    @Test fun unknownSsidOrPlaceholderBssidIsUnknownNeverNotHome() {
        val saved = listOf(HomeNetwork("HomeNet", "aa:bb:cc:dd:ee:01"))
        assertNull(PresenceSignals.isHome(WifiId("<unknown ssid>", "02:00:00:00:00:00"), saved))
        assertNull(PresenceSignals.isHome(WifiId("\"<unknown ssid>\"", null), saved))
        assertNull(PresenceSignals.isHome(WifiId("", "  "), saved))
    }

    @Test fun aSavedNetworkWithoutBssidMatchesBySsidDespiteADifferentBssid() {
        val saved = listOf(HomeNetwork("HomeNet", null))
        assertEquals(true, PresenceSignals.isHome(WifiId("HomeNet", "11:22:33:44:55:66"), saved))
    }

    @Test fun anyOfSeveralSavedNetworksOrCarsMatches() {
        val homes = listOf(HomeNetwork("Office", "aa:aa:aa:aa:aa:01"), HomeNetwork("HomeNet", "bb:bb:bb:bb:bb:02"))
        assertEquals(true, PresenceSignals.isHome(WifiId("HomeNet", null), homes))
        val cars = listOf(CarDevice("Subaru", "AA:01"), CarDevice("Truck", "BB:02"))
        assertEquals(true, PresenceSignals.carConnected(setOf("bb:02"), cars))
    }

    @Test fun aUsableNetworkHasACleanSsidOrIsNull() {
        assertEquals(WifiId("HomeNet", "aa:01"), PresenceSignals.usableNetwork(WifiId("\"HomeNet\"", "aa:01")))
        assertNull(PresenceSignals.usableNetwork(WifiId("<unknown ssid>", "02:00:00:00:00:00")))
        assertNull(PresenceSignals.usableNetwork(WifiId(null, "aa:01")))
        assertNull(PresenceSignals.usableNetwork(null))
    }

    @Test fun theChipSaysWhatIsHappeningAndIsPlainWhenNothingIsConfigured() {
        assertEquals("At home, paused", PresenceText.chip(PresenceState.AtHome, configured = true))
        assertEquals("In car, not counting", PresenceText.chip(PresenceState.InCar, configured = true))
        assertEquals("Outside zones, saving battery", PresenceText.chip(PresenceState.OutsideZones, configured = true))
        assertEquals("Tracking", PresenceText.chip(PresenceState.InZone, configured = true))
        assertEquals("Protection off", PresenceText.chip(PresenceState.OutsideZones, configured = false))
        assertEquals("Not playing", PresenceText.chip(PresenceState.Stopped, configured = true))
    }
}
