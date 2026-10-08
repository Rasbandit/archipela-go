package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChoicesTest {
    private fun home(
        ssid: String,
        bssid: String? = null,
    ) = HomeNetwork(ssid, bssid)

    private fun ssids(l: List<WifiChoice>) = l.map { it.ssid }

    @Test fun connectedFirstThenSavedThenNearbyAlphabetical() {
        val r = WifiChoices.merge(listOf(home("Saved")), WifiId("\"Now\"", "aa:bb"), listOf("zeta", "Alpha"), "")
        assertEquals(listOf("Now", "Saved", "Alpha", "zeta"), ssids(r))
    }

    @Test fun sameNameIsListedOnce() {
        val r = WifiChoices.merge(listOf(home("Home")), WifiId("Home", "aa:bb"), listOf("Home", "Home", "Other"), "")
        assertEquals(listOf("Home", "Other"), ssids(r))
    }

    @Test fun hiddenAndUnknownNamesAreDropped() {
        val r = WifiChoices.merge(emptyList(), null, listOf("", "  ", "<unknown ssid>", "\"\"", "Real"), "")
        assertEquals(listOf("Real"), ssids(r))
    }

    @Test fun savedNetworkOutOfRangeIsStillListedAndTicked() {
        val r = WifiChoices.merge(listOf(home("Away5G")), null, emptyList(), "")
        assertEquals(1, r.size)
        assertTrue(r[0].saved)
        assertTrue(!r[0].connected)
    }

    @Test fun emptyScanAndNothingSavedIsAnEmptyList() {
        assertEquals(emptyList<WifiChoice>(), WifiChoices.merge(emptyList(), null, emptyList(), ""))
    }

    @Test fun queryFiltersCaseInsensitively() {
        val r = WifiChoices.merge(emptyList(), null, listOf("Kitchen-2G", "Kitchen-5G", "Neighbour"), "  kitchen ")
        assertEquals(listOf("Kitchen-2G", "Kitchen-5G"), ssids(r))
    }

    @Test fun queryThatMatchesNothingIsEmpty() {
        assertEquals(emptyList<WifiChoice>(), WifiChoices.merge(emptyList(), null, listOf("A"), "zzz"))
    }

    @Test fun onlyTheConnectedNetworkCarriesABssid() {
        val r = WifiChoices.merge(emptyList(), WifiId("Now", "aa:bb"), listOf("Other"), "")
        assertEquals("aa:bb", r[0].bssid)
        assertNull(r[1].bssid)
        assertTrue(r[0].connected)
    }

    @Test fun savedBssidIsKept() {
        assertEquals("cc:dd", WifiChoices.merge(listOf(home("Home", "cc:dd")), null, emptyList(), "")[0].bssid)
    }

    @Test fun savedAndConnectedNetworkIsListedOnceWithTheCurrentBssid() {
        val r = WifiChoices.merge(listOf(home("Home", "old:bb")), WifiId("Home", "new:bb"), listOf("Home"), "")
        assertEquals(1, r.size)
        assertTrue(r[0].saved)
        assertTrue(r[0].connected)
        assertEquals("new:bb", r[0].bssid)
    }

    private fun car(
        name: String,
        addr: String,
    ) = CarDevice(name, addr)

    @Test fun pairedCarsComeFirstAndSavedUnpairedStayListed() {
        val r = CarChoices.merge(listOf(car("Buds", "A1")), listOf(car("Old car", "B2"), car("Buds", "a1")), "")
        assertEquals(listOf("Buds", "Old car"), r.map { it.name })
    }

    @Test fun carQueryFiltersByNameCaseInsensitively() {
        val r = CarChoices.merge(listOf(car("Honda Civic", "A"), car("Buds", "B")), emptyList(), "CIV")
        assertEquals(listOf("Honda Civic"), r.map { it.name })
    }

    @Test fun carQueryAlsoFiltersSavedUnpairedDevices() {
        val r = CarChoices.merge(listOf(car("Buds", "B")), listOf(car("Old car", "C"), car("Van", "D")), "van")
        assertEquals(listOf("Van"), r.map { it.name })
    }
}
