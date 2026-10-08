package dev.apgo2.ui

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import java.util.Locale

private const val M_PER_MILE = 1609.344

class UnitsTest {
    private lateinit var saved: Locale

    @Before fun save() {
        saved = Locale.getDefault()
    }

    @After fun restore() {
        Locale.setDefault(saved)
    }

    private fun metric() = Locale.setDefault(Locale.CANADA)

    private fun imperial() = Locale.setDefault(Locale.US)

    @Test fun wholeKilometresKeepTheirZeros() {
        metric()
        assertEquals("1 km", Units.distance(1_000.0))
        assertEquals("100 km", Units.distance(100_000.0))
        assertEquals("1000 km", Units.distance(1_000_000.0))
        assertEquals("10000 km", Units.distance(10_000_000.0))
    }

    @Test fun wholeMilesKeepTheirZeros() {
        imperial()
        assertEquals("1 mi", Units.distance(M_PER_MILE))
        assertEquals("100 mi", Units.distance(100 * M_PER_MILE))
        assertEquals("1000 mi", Units.distance(1_000 * M_PER_MILE))
        assertEquals("10000 mi", Units.distance(10_000 * M_PER_MILE))
    }

    @Test fun trailingFractionalZerosAreDropped() {
        metric()
        assertEquals("1.5 km", Units.distance(1_500.0))
        assertEquals("1.25 km", Units.distance(1_250.0))
        assertEquals("10 km", Units.distance(10_000.0))
        assertEquals("12.5 km", Units.distance(12_500.0))
        imperial()
        assertEquals("1.5 mi", Units.distance(1.5 * M_PER_MILE))
    }

    @Test fun roundingUpToAHundredDropsTheFraction() {
        metric()
        assertEquals("100 km", Units.distance(99_960.0))
    }

    @Test fun roundingUpToTenDropsTheFraction() {
        metric()
        assertEquals("10 km", Units.distance(9_996.0))
    }

    @Test fun zeroReadsInTheSmallUnit() {
        metric()
        assertEquals("0 m", Units.distance(0.0))
        imperial()
        assertEquals("0 ft", Units.distance(0.0))
    }

    @Test fun metresSwitchToKilometresAtOneKilometre() {
        metric()
        assertEquals("999 m", Units.distance(999.0))
        assertEquals("1 km", Units.distance(1_000.0))
    }

    @Test fun justUnderAKilometreRoundsUpToKilometresNotOneThousandMetres() {
        metric()
        assertEquals("999 m", Units.distance(999.4))
        assertEquals("1 km", Units.distance(999.6))
    }

    @Test fun percentRoundsToAWholeNumber() {
        assertEquals("38%", Units.percent(0.375))
        assertEquals("0%", Units.percent(0.0))
        assertEquals("100%", Units.percent(1.0))
    }

    @Test fun percentUsesWesternDigitsInEveryLocale() {
        Locale.setDefault(Locale.forLanguageTag("ar-EG"))
        assertEquals("38%", Units.percent(0.375))
        assertEquals("1.5 km", Units.distance(1_500.0))
    }

    @Test fun feetSwitchToMilesAtATenthOfAMile() {
        imperial()
        assertEquals("525 ft", Units.distance(160.0))
        assertEquals("0.1 mi", Units.distance(0.1 * M_PER_MILE))
    }

    @Test fun decimalCommaLocaleStillUsesAPoint() {
        Locale.setDefault(Locale.GERMANY)
        assertEquals("1 km", Units.distance(1_000.0))
        assertEquals("1.5 km", Units.distance(1_500.0))
        assertEquals("100 km", Units.distance(100_000.0))
        assertEquals("2.50 km²", Units.area(2_500_000.0))
    }

    @Test fun areaUsesTheRegionsUnit() {
        metric()
        assertEquals("2.50 km²", Units.area(2_500_000.0))
        imperial()
        assertEquals("1.00 mi²", Units.area(M_PER_MILE * M_PER_MILE))
    }
}
