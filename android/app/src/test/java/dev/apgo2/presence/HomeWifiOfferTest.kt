package dev.apgo2.presence

import dev.apgo2.ui.distanceM
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// Stands in for the core's nearest-camp lookup (`Engine.nearestCamp`, tested in the core): the nearest camp within 75 m.
private fun nearOf(camps: List<OfferCamp>): (GeoFix) -> OfferCamp? =
    { f -> camps.filter { distanceM(f, it.pin) <= 75.0 }.minByOrNull { distanceM(f, it.pin) } }

class HomeWifiOfferTest {
    private val home = GeoFix(lat = 47.0, lon = 8.0, accuracyM = 0.0)

    private val homeCamp = OfferCamp("home", "Home", home)

    private val now = 1_000_000_000L

    private val base =
        OfferSignals(
            saved = emptyList(),
            playing = true,
            fix = GeoFix(home.lat, home.lon, accuracyM = 10.0),
            fixAgeMs = 1_000L,
            campNear = nearOf(listOf(homeCamp)),
            wifi = WifiId("\"HomeNet\"", "aa:bb:cc:dd:ee:01"),
            muted = emptySet(),
            showing = false,
            dismissedAtMs = null,
            nowMs = now,
        )

    // 1 m north of home is about 1 / 111 195 degrees of latitude.
    private fun north(m: Double) = home.lat + m / 111_195.0

    @Test fun offersTheCleanedNetworkWhenEveryConditionHolds() {
        assertEquals(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), HomeWifiOffer.decide(base)?.net)
    }

    @Test fun noOfferOnceAHomeNetworkIsSaved() {
        assertNull(HomeWifiOffer.decide(base.copy(saved = listOf(HomeNetwork("Other", null)))))
    }

    @Test fun noOfferWithoutAGame() {
        assertNull(HomeWifiOffer.decide(base.copy(playing = false)))
    }

    @Test fun noOfferWithoutAFixOrAHomePin() {
        assertNull(HomeWifiOffer.decide(base.copy(fix = null)))
        assertNull(HomeWifiOffer.decide(base.copy(campNear = nearOf(emptyList()))))
    }

    @Test fun theFixMustBeWithin75mOfHome() {
        assertEquals(
            WifiId("HomeNet", "aa:bb:cc:dd:ee:01"),
            HomeWifiOffer.decide(base.copy(fix = GeoFix(north(74.9), home.lon, 10.0)))?.net,
        )
        assertNull(HomeWifiOffer.decide(base.copy(fix = GeoFix(north(75.1), home.lon, 10.0))))
        assertNull(HomeWifiOffer.decide(base.copy(fix = GeoFix(north(5_000.0), home.lon, 10.0))))
    }

    @Test fun theFixMustBeAccurateTo50m() {
        assertEquals(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), HomeWifiOffer.decide(base.copy(fix = GeoFix(home.lat, home.lon, 50.0)))?.net)
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
            HomeWifiOffer.decide(base.copy(muted = setOf("CafeWifi")))?.net,
        )
    }

    @Test fun noSecondOfferWhileOneIsShowing() {
        assertNull(HomeWifiOffer.decide(base.copy(showing = true)))
    }

    @Test fun laterWaitsTenMinutes() {
        val tenMin = 10 * 60 * 1000L
        assertNull(HomeWifiOffer.decide(base.copy(dismissedAtMs = now)))
        assertNull(HomeWifiOffer.decide(base.copy(dismissedAtMs = now - tenMin + 1)))
        assertEquals(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), HomeWifiOffer.decide(base.copy(dismissedAtMs = now - tenMin))?.net)
    }

    @Test fun onlyAFixFromTheLastTwoMinutesCounts() {
        val offered = WifiId("HomeNet", "aa:bb:cc:dd:ee:01")
        assertEquals(offered, HomeWifiOffer.decide(base.copy(fixAgeMs = 119_000L))?.net)
        assertEquals("exactly 120 s still counts", offered, HomeWifiOffer.decide(base.copy(fixAgeMs = 120_000L))?.net)
        assertNull(HomeWifiOffer.decide(base.copy(fixAgeMs = 120_001L)))
        assertNull(HomeWifiOffer.decide(base.copy(fixAgeMs = 121_000L)))
        assertNull("a cached last-known fix from hours ago", HomeWifiOffer.decide(base.copy(fixAgeMs = 3 * 3_600_000L)))
        assertNull("no timestamp", HomeWifiOffer.decide(base.copy(fixAgeMs = null)))
        assertNull("a future-stamped fix (negative age)", HomeWifiOffer.decide(base.copy(fixAgeMs = -1L)))
    }

    @Test fun aRealNameInQuotesStaysMutedAfterTheRoundTrip() {
        val quoted = base.copy(wifi = WifiId("\"\"Net\"\"", null))
        val offer = HomeWifiOffer.decide(quoted)
        assertEquals(WifiId("\"Net\"", null), offer?.net)
        val muted = HomeWifiOffer.mute(emptySet(), offer!!.net.ssid!!)
        assertNull(HomeWifiOffer.decide(quoted.copy(muted = muted)))
    }

    @Test fun aNewOfferIsShownWhenNoneIsUp() {
        assertEquals(Offer(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), homeCamp), HomeWifiOffer.next(null, base))
        assertNull(HomeWifiOffer.next(null, base.copy(playing = false)))
    }

    @Test fun theShowingOfferIsKeptWhileItStillHolds() {
        val showing = Offer(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), homeCamp)
        assertEquals(showing, HomeWifiOffer.next(showing, base.copy(showing = true)))
        assertEquals(
            "same network, other access point: keep the one on screen",
            showing,
            HomeWifiOffer.next(showing, base.copy(wifi = WifiId("HomeNet", "aa:bb:cc:dd:ee:02"))),
        )
    }

    @Test fun theShowingOfferIsClearedWhenItNoLongerHolds() {
        val showing = Offer(WifiId("HomeNet", "aa:bb:cc:dd:ee:01"), homeCamp)
        assertNull("game closed", HomeWifiOffer.next(showing, base.copy(playing = false)))
        assertNull("Wi-Fi lost", HomeWifiOffer.next(showing, base.copy(wifi = null)))
        assertNull("walked away", HomeWifiOffer.next(showing, base.copy(fix = GeoFix(north(500.0), home.lon, 10.0))))
        assertNull("saved in setup", HomeWifiOffer.next(showing, base.copy(saved = listOf(HomeNetwork("HomeNet", null)))))
        assertNull("another network", HomeWifiOffer.next(showing, base.copy(wifi = WifiId("CafeWifi", null))))
    }

    @Test fun offersAtTheNearestCampAndSavesItsId() {
        val cabin = OfferCamp("cabin", "Cabin", GeoFix(north(60.0), home.lon, 0.0))
        val s = base.copy(campNear = nearOf(listOf(OfferCamp("home", "Home", home), cabin)), fix = GeoFix(north(55.0), home.lon, 10.0))
        assertEquals(cabin, HomeWifiOffer.nearestCamp(s))
        assertNull("beyond 75 m of every camp", HomeWifiOffer.nearestCamp(s.copy(fix = GeoFix(north(500.0), home.lon, 10.0))))
    }

    @Test fun aCampWithItsOwnNetworkIsNotOfferedAgainButAnotherCampIs() {
        val cabin = OfferCamp("cabin", "Cabin", GeoFix(north(2000.0), home.lon, 0.0))
        val atCabin =
            base.copy(
                campNear = nearOf(listOf(OfferCamp("home", "Home", home), cabin)),
                fix = GeoFix(cabin.pin.lat, home.lon, 10.0),
            )
        val homeSaved = listOf(HomeNetwork("HomeNet2", null, "home"))
        assertEquals(
            "cabin has none yet",
            WifiId("HomeNet", "aa:bb:cc:dd:ee:01"),
            HomeWifiOffer.decide(atCabin.copy(saved = homeSaved))?.net,
        )
        assertNull("cabin has one", HomeWifiOffer.decide(atCabin.copy(saved = homeSaved + HomeNetwork("CabinNet", null, "cabin"))))
        assertNull(
            "this network is already a camp's",
            HomeWifiOffer.decide(atCabin.copy(saved = listOf(HomeNetwork("HomeNet", null, "home")))),
        )
    }

    @Test fun theOfferCarriesTheCampItIsFor() {
        assertEquals(homeCamp, HomeWifiOffer.decide(base)?.camp)
    }

    @Test fun aNetworkSavedForAnUnknownCampIsIgnoredWhenFilteredToKnownCamps() {
        val saved = listOf(HomeNetwork("HomeNet", null, "gone"))
        assertEquals(emptyList<HomeNetwork>(), PresenceSignals.forCamps(saved, setOf("home")))
        assertNull(PresenceSignals.campOf(WifiId("HomeNet", null), PresenceSignals.forCamps(saved, setOf("home"))))
        assertEquals(
            WifiId("HomeNet", "aa:bb:cc:dd:ee:01"),
            HomeWifiOffer.decide(base.copy(saved = PresenceSignals.forCamps(saved, setOf("home"))))?.net,
        )
    }

    @Test fun onlyAFreshAccurateFixCanSayWhichCampThePlayerIsAt() {
        val fix = GeoFix(home.lat, home.lon, 10.0)
        assertEquals(fix, HomeWifiOffer.usableFix(fix, 1_000))
        assertNull("a cached old location", HomeWifiOffer.usableFix(fix, HomeWifiOffer.MAX_FIX_AGE_MS + 1))
        assertNull("stamped in the future", HomeWifiOffer.usableFix(fix, -1))
        assertNull("no age", HomeWifiOffer.usableFix(fix, null))
        assertNull("too blurry", HomeWifiOffer.usableFix(fix.copy(accuracyM = HomeWifiOffer.MAX_ACCURACY_M + 1), 1_000))
    }
}
