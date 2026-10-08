package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class AwaySettingsTest {
    @Test fun automaticMeansZero() = assertEquals(0u, AwaySettings.distance(auto = true, text = "1500"))

    @Test fun aCustomDistanceIsTheTypedNumber() = assertEquals(1500u, AwaySettings.distance(auto = false, text = "1500"))

    @Test fun nonsenseOrEmptyTextFallsBackToTheDefaultKilometre() {
        assertEquals(1000u, AwaySettings.distance(auto = false, text = ""))
        assertEquals(1000u, AwaySettings.distance(auto = false, text = "abc"))
        assertEquals(1000u, AwaySettings.distance(auto = false, text = "0"))
    }

    @Test fun hugeNumbersAreCappedAtTwentyKilometres() = assertEquals(20_000u, AwaySettings.distance(auto = false, text = "999999999"))
}
