package dev.apgo2

import com.google.android.gms.location.Granularity
import com.google.android.gms.location.Priority
import dev.apgo2.presence.Decision
import dev.apgo2.presence.GpsMode
import dev.apgo2.presence.PresenceState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GpsPolicyTest {
    private val inZone = Decision(PresenceState.InZone, GpsMode.Rate(5_000L, 0f), counting = true)

    @Test fun inAZoneTheScreenPicksOneOrFiveSecondsAndBothUseGpsQuality() {
        for (visible in listOf(true, false)) {
            assertEquals(
                "on, visible=$visible",
                GpsPolicy.Rate(1_000L, 0f, highAccuracy = true),
                GpsPolicy.forDecision(inZone, visible, screenOn = true),
            )
            assertEquals(
                "off, visible=$visible",
                GpsPolicy.Rate(5_000L, 0f, highAccuracy = true, maxDelayMs = 10_000L),
                GpsPolicy.forDecision(inZone, visible, screenOn = false),
            )
        }
    }

    @Test fun insideAZoneTheChipNeverDropsToBalanced() {
        for (visible in listOf(true, false)) {
            for (screen in listOf(true, false)) {
                assertTrue(GpsPolicy.forDecision(inZone, visible, screen)!!.highAccuracy)
            }
        }
    }

    @Test fun outsideEveryZoneTheCoarseRateIgnoresTheScreen() {
        val d = Decision(PresenceState.OutsideZones, GpsMode.Rate(90_000L, 0f), counting = true)
        for (screen in listOf(true, false)) {
            assertEquals(GpsPolicy.Rate(90_000L, 0f, highAccuracy = false), GpsPolicy.forDecision(d, appVisible = false, screenOn = screen))
        }
    }

    @Test fun stoppedUsesTheIdleRuleOnlyWhileTheAppIsVisible() {
        val d = Decision(PresenceState.Stopped, GpsMode.Off, counting = false)
        assertEquals(GpsPolicy.IDLE, GpsPolicy.forDecision(d, appVisible = true, screenOn = true))
        assertNull(GpsPolicy.forDecision(d, appVisible = false, screenOn = true))
    }

    @Test fun offMeansNoLocationEvenWhenTheAppIsOnScreen() {
        assertNull(GpsPolicy.forDecision(Decision(PresenceState.AtHome, GpsMode.Off, counting = false), appVisible = true, screenOn = true))
        assertNull(GpsPolicy.forDecision(Decision(PresenceState.InCar, GpsMode.Off, counting = false), appVisible = true, screenOn = true))
    }

    @Test fun playingStaysFarUnderTheCoreGapLimit() {
        // The core resets the filter after 5 minutes without a fix: even screen-off batches must be far below that.
        val off = GpsPolicy.inZone(screenOn = false)
        assertTrue(off.intervalMs + off.maxDelayMs < 60_000L)
    }

    @Test fun theRequestCarriesIntervalBatchingAndQuality() {
        assertEquals(GpsPolicy.RequestSpec(1_000L, 1_000L, 0L, 0f, 100), GpsPolicy.request(GpsPolicy.inZone(screenOn = true)))
        assertEquals(GpsPolicy.RequestSpec(5_000L, 5_000L, 10_000L, 0f, 100), GpsPolicy.request(GpsPolicy.inZone(screenOn = false)))
        assertEquals(102, GpsPolicy.request(GpsPolicy.IDLE).quality)
    }
}

class GpsProvidersTest {
    private val all = setOf("fused", "gps", "network", "passive")

    @Test fun withPlayServicesGooglesFusedLocationIsUsedOnEveryAndroidWithItsOwnColdStart() {
        val gms = GpsPolicy.ProviderPlan("gms-fused", "gms-fused")
        assertEquals(gms, GpsPolicy.providers(all, gms = true))
        // Android 8-11 lists no LocationManager fused provider; Play services still fuses.
        assertEquals(gms, GpsPolicy.providers(setOf("gps", "network", "passive"), gms = true))
        assertEquals(gms, GpsPolicy.providers(setOf("network"), gms = true))
    }

    @Test fun withPlayServicesButLocationOffNothingIsAsked() {
        assertEquals(GpsPolicy.ProviderPlan(null, null), GpsPolicy.providers(setOf("passive"), gms = true))
        assertEquals(GpsPolicy.ProviderPlan(null, null), GpsPolicy.providers(emptySet(), gms = true))
    }

    @Test fun withoutPlayServicesTheGpsProviderIsUsedAndNetworkOnlyForTheColdStart() {
        // AOSP's fused provider only picks between gps and network: our filter does the fusion.
        assertEquals(GpsPolicy.ProviderPlan("gps", "network"), GpsPolicy.providers(all, gms = false))
        assertEquals(GpsPolicy.ProviderPlan("gps", null), GpsPolicy.providers(setOf("fused", "gps"), gms = false))
    }

    @Test fun withoutPlayServicesNetworkIsTheLastResortAndNothingEnabledMeansNothing() {
        assertEquals(GpsPolicy.ProviderPlan("network", null), GpsPolicy.providers(setOf("network"), gms = false))
        assertEquals(GpsPolicy.ProviderPlan(null, null), GpsPolicy.providers(setOf("fused", "passive"), gms = false))
    }
}

class GmsRequestTest {
    @Test fun inAZoneEitherScreenWaitsForAnAccurateFineFix() {
        for (screen in listOf(true, false)) {
            val r = GpsPolicy.gmsRequest(GpsPolicy.inZone(screenOn = screen))
            assertTrue("screen=$screen", r.isWaitForAccurateLocation)
            assertEquals("screen=$screen", Granularity.GRANULARITY_FINE, r.granularity)
        }
    }

    @Test fun theIdleMarkerDoesNotWaitAndKeepsThePermissionGranularity() {
        val r = GpsPolicy.gmsRequest(GpsPolicy.IDLE)
        assertFalse(r.isWaitForAccurateLocation)
        assertEquals(Granularity.GRANULARITY_PERMISSION_LEVEL, r.granularity)
    }

    @Test fun screenOnInAZoneIsHighAccuracyEverySecondUnbatched() {
        val r = GpsPolicy.gmsRequest(GpsPolicy.inZone(screenOn = true))
        assertEquals(Priority.PRIORITY_HIGH_ACCURACY, r.priority)
        assertEquals(1_000L, r.intervalMillis)
        assertEquals(1_000L, r.minUpdateIntervalMillis)
        // Play services reports an unbatched request's delay as the interval; isBatched is the plain answer.
        assertFalse(r.isBatched)
        assertEquals(0f, r.minUpdateDistanceMeters)
    }

    @Test fun screenOffInAZoneIsHighAccuracyEveryFiveSecondsBatchedUpToTen() {
        val r = GpsPolicy.gmsRequest(GpsPolicy.inZone(screenOn = false))
        assertEquals(Priority.PRIORITY_HIGH_ACCURACY, r.priority)
        assertEquals(5_000L, r.intervalMillis)
        assertEquals(5_000L, r.minUpdateIntervalMillis)
        assertTrue(r.isBatched)
        assertEquals(10_000L, r.maxUpdateDelayMillis)
    }

    @Test fun theIdleMarkerIsBalancedWithItsDistanceFilter() {
        val r = GpsPolicy.gmsRequest(GpsPolicy.IDLE)
        assertEquals(Priority.PRIORITY_BALANCED_POWER_ACCURACY, r.priority)
        assertEquals(15_000L, r.intervalMillis)
        assertEquals(15_000L, r.minUpdateIntervalMillis)
        assertEquals(20f, r.minUpdateDistanceMeters)
        assertFalse(r.isBatched)
    }

    @Test fun theColdStartAsksTheChipOnceForAFixAtMostTwoMinutesOld() {
        val r = GpsPolicy.gmsColdStart()
        assertEquals(Priority.PRIORITY_HIGH_ACCURACY, r.priority)
        assertEquals(120_000L, r.maxUpdateAgeMillis)
    }

    @Test fun noLocationWhileAGameWaitsToLearnWhetherYouAreHome() {
        // At start-up the decision is still Stopped while Wi-Fi is read; the "show my dot" rule must not wake GPS for a game then.
        val d = Decision(PresenceState.Stopped, GpsMode.Off, counting = false)
        assertEquals("holding", null, GpsPolicy.forDecision(d, appVisible = true, screenOn = true, holding = true))
        assertEquals("not holding", GpsPolicy.IDLE, GpsPolicy.forDecision(d, appVisible = true, screenOn = true, holding = false))
    }
}
