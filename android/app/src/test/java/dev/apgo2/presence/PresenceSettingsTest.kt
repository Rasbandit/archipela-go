package dev.apgo2.presence

import android.content.Context
import androidx.compose.runtime.snapshots.Snapshot
import androidx.core.content.edit
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.apgo2.Diag
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(AndroidJUnit4::class)
@Config(sdk = [35]) // the newest Android version Robolectric runs
class PresenceSettingsTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()

    private fun fresh(json: String? = null): PresenceSettings {
        ctx.getSharedPreferences("presence", Context.MODE_PRIVATE).edit(commit = true) {
            clear()
            json?.let { putString("home", it) }
        }
        return PresenceSettings(ctx)
    }

    @Test fun removeCampPrunesThatCampsNetworksAndKeepsTheRest() {
        val s = fresh()
        s.addHome(HomeNetwork("A1", null, "a"))
        s.addHome(HomeNetwork("A2", "aa:bb", "a"))
        s.addHome(HomeNetwork("B1", null, "b"))
        assertEquals(2, s.networkCount("a"))
        s.removeCamp("a")
        assertEquals(0, s.networkCount("a"))
        assertEquals(listOf("B1"), s.homeNetworks.map { it.ssid })
        assertEquals(1, PresenceSettings(ctx).networkCount("b"))
        assertEquals("the prune is saved", 1, PresenceSettings(ctx).homeNetworks.size)
    }

    @Test fun networksSavedBeforeCampsBelongToHomeAndGoWithTheHomeCamp() {
        val s = fresh("""[{"ssid":"Old","bssid":""},{"ssid":"Other","bssid":"","campId":"x"}]""")
        assertEquals(1, s.networkCount("home"))
        s.removeCamp("home")
        assertEquals(0, s.networkCount("home"))
        assertEquals(listOf("Other"), s.homeNetworks.map { it.ssid })
    }

    @Test fun anUnknownCampChangesNothing() {
        val s = fresh()
        s.addHome(HomeNetwork("A", null, "a"))
        s.removeCamp("zzz")
        assertEquals(1, s.homeNetworks.size)
    }

    @Test fun theNetworksAreObservableSoTheCampListRefreshes() {
        val s = fresh()
        var reads = 0
        val handle = Snapshot.registerApplyObserver { _, _ -> reads++ }
        try {
            Snapshot.withMutableSnapshot { s.addHome(HomeNetwork("A", null, "a")) }
        } finally {
            handle.dispose()
        }
        assertTrue("saving a network is a state write", reads > 0)
    }

    @Test fun oneNetworkCanServeTwoCampsAndRemovingItFromOneKeepsTheOther() {
        val s = fresh()
        s.addHome(HomeNetwork("Shared", null, "a"))
        s.addHome(HomeNetwork("Shared", "aa:bb", "b"))
        assertEquals(1, s.networkCount("a"))
        assertEquals(1, s.networkCount("b"))
        s.addHome(HomeNetwork("\"Shared\"", "cc:dd", "a"))
        assertEquals("saving it again for the same camp replaces it", 1, s.networkCount("a"))
        s.removeHome("Shared", "a")
        assertEquals(0, s.networkCount("a"))
        assertEquals(listOf(HomeNetwork("Shared", "aa:bb", "b")), PresenceSettings(ctx).homeNetworks)
    }

    @Test fun unreadableSavedNetworksAreLoggedNotSilentlyDropped() {
        Diag.init(ctx)
        val s = fresh("not json")
        assertEquals(emptyList<HomeNetwork>(), s.homeNetworks)
        val logged =
            java.io
                .File(ctx.filesDir, "diag")
                .listFiles()
                .orEmpty()
                .joinToString("\n") { it.readText() }
        assertTrue(logged, logged.contains("saved home networks unreadable"))
    }

    @Test fun aNetworkTickedInSetupIsSavedForTheChosenCamp() {
        val s = fresh()
        s.setHome("Net", "aa:bb", "cabin", on = true)
        assertEquals(listOf(HomeNetwork("Net", "aa:bb", "cabin")), s.homeNetworks)
        s.setHome("Net", null, "home", on = true)
        s.setHome("Net", null, "cabin", on = false)
        assertEquals(listOf(HomeNetwork("Net", null, "home")), PresenceSettings(ctx).homeNetworks)
    }
}
