package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class WifiCampTest {
    private val camps = listOf("home", "cabin", "lake")

    @Test fun theCampTheCoreFindsNearIsTheDefault() {
        assertEquals("cabin", WifiCamp.pick(camps, preselect = null, near = "cabin"))
    }

    @Test fun withNoCampNearTheFirstCampIsTheDefault() {
        assertEquals("home", WifiCamp.pick(camps, preselect = null, near = null))
        assertEquals("a near camp that is gone", "home", WifiCamp.pick(camps, preselect = null, near = "gone"))
    }

    @Test fun aCampChosenOnTheCampsScreenWinsOverTheNearOne() {
        assertEquals("lake", WifiCamp.pick(camps, preselect = "lake", near = "cabin"))
        assertEquals("a deleted camp is not kept", "cabin", WifiCamp.pick(camps, preselect = "gone", near = "cabin"))
    }

    @Test fun noCampsNoChoice() {
        assertNull(WifiCamp.pick(emptyList(), preselect = "home", near = "home"))
    }

    @Test fun theListIsTickedForTheChosenCampOnly() {
        val saved = listOf(HomeNetwork("A", null, "home"), HomeNetwork("B", null, "cabin"), HomeNetwork("A", null, "cabin"))
        assertEquals(listOf("B", "A"), WifiCamp.savedFor(saved, "cabin").map { it.ssid })
        assertEquals(emptyList<HomeNetwork>(), WifiCamp.savedFor(saved, null))
    }
}
