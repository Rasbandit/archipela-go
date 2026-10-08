package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HomeWifiOfferTest {
    private val home = GeoFix(lat = 47.0, lon = 8.0, accuracyM = 0.0)

    private val now = 1_000_000_000L

    private val base =
        OfferSignals(
            saved = emptyList(),
            playing = true,
            fix = GeoFix(home.lat, home.lon, accuracyM = 10.0),
            fixAtMs = now,
            home = home,
            wifi = WifiId("\"HomeNet\"", "aa:bb:cc:dd:ee:01"),
            muted = emptySet(),
            showing = false,
            dismissedAtMs = null,
            nowMs = now,
        )

    // 1 m north of home is about 1 / 111 195 degrees of latitude.
    private fun north(m: Double) = home.lat + m / 111_195.0

    @Test fun offersTheCleanedNetworkWhenEveryConditionHolds() {
        assertEquals(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), HomeWifiOffer.decide(base))
    }

    @Test fun noOfferOnceAHomeNetworkIsSaved() {
        assertNull(HomeWifiOffer.decide(base.copy(saved = listOf(HomeNetwork("Other", null)))))
    }

    @Test fun noOfferWithoutAGame() {
        assertNull(HomeWifiOffer.decide(base.copy(playing = false)))
    }

    @Test fun noOfferWithoutAFixOrAHomePin() {
        assertNull(HomeWifiOffer.decide(base.copy(fix = null)))
        assertNull(HomeWifiOffer.decide(base.copy(home = null)))
    }

    @Test fun theFixMustBeWithin75mOfHome() {
        assertEquals(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), HomeWifiOffer.decide(base.copy(fix = GeoFix(north(74.9), home.lon, 10.0))))
        assertNull(HomeWifiOffer.decide(base.copy(fix = GeoFix(north(75.1), home.lon, 10.0))))
        assertNull(HomeWifiOffer.decide(base.copy(fix = GeoFix(north(5_000.0), home.lon, 10.0))))
    }

    @Test fun theFixMustBeAccurateTo50m() {
        assertEquals(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), HomeWifiOffer.decide(base.copy(fix = GeoFix(home.lat, home.lon, 50.0))))
        assertNull(HomeWifiOffer.decide(base.copy(fix = GeoFix(home.lat, home.lon, 50.1))))
    }

    @Test fun noOfferWithoutAUsableNetworkName() {
        assertNull(HomeWifiOffer.decide(base.copy(wifi = null)))
        assertNull(HomeWifiOffer.decide(base.copy(wifi = WifiId(null, "aa:bb:cc:dd:ee:01"))))
        assertNull(HomeWifiOffer.decide(base.copy(wifi = WifiId("", null))))
        assertNull(HomeWifiOffer.decide(base.copy(wifi = WifiId("<unknown ssid>", "02:00:00:00:00:00"))))
        assertNull(HomeWifiOffer.decide(base.copy(wifi = WifiId("\"<unknown ssid>\"", null))))
    }

    @Test fun aMutedNetworkIsNeverOffered() {
        assertNull(HomeWifiOffer.decide(base.copy(muted = setOf("HomeNet"))))
        assertEquals(
            "another network is still offered",
            WifiId("HomeNet", "aa:bb:cc:dd:ee:01"),
            HomeWifiOffer.decide(base.copy(muted = setOf("CafeWifi"))),
        )
    }

    @Test fun noSecondOfferWhileOneIsShowing() {
        assertNull(HomeWifiOffer.decide(base.copy(showing = true)))
    }

    @Test fun laterWaitsTenMinutes() {
        val tenMin = 10 * 60 * 1000L
        assertNull(HomeWifiOffer.decide(base.copy(dismissedAtMs = now)))
        assertNull(HomeWifiOffer.decide(base.copy(dismissedAtMs = now - tenMin + 1)))
        assertEquals(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), HomeWifiOffer.decide(base.copy(dismissedAtMs = now - tenMin)))
    }

    @Test fun onlyAFixFromTheLastTwoMinutesCounts() {
        val sec = 1000L
        assertEquals(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), HomeWifiOffer.decide(base.copy(fixAtMs = now - 119 * sec)))
        assertNull(HomeWifiOffer.decide(base.copy(fixAtMs = now - 121 * sec)))
        assertNull("a cached last-known fix from hours ago", HomeWifiOffer.decide(base.copy(fixAtMs = now - 3 * 3_600 * sec)))
        assertNull("no timestamp", HomeWifiOffer.decide(base.copy(fixAtMs = null)))
    }

    @Test fun distanceIsMeasuredInMetres() {
        assertEquals(0.0, HomeWifiOffer.distanceM(home, home), 1e-9)
        assertEquals(100.0, HomeWifiOffer.distanceM(home, GeoFix(north(100.0), home.lon, 0.0)), 0.1)
    }
}
