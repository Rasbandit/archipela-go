package dev.apgo2.ui

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.apgo_ffi.UnitSystem
import java.util.Locale

private const val M_PER_MILE = 1609.344

// The rounding rules themselves are tested in core/src/units.rs; these check the wrapper reaches the core with the chosen units.
class UnitsTest {
    private lateinit var saved: Locale
    private lateinit var savedSystem: UnitSystem

    @Before fun save() {
        saved = Locale.getDefault()
        savedSystem = Units.system
    }

    @After fun restore() {
        Locale.setDefault(saved)
        Units.system = savedSystem
    }

    @Test fun distancesFollowTheChosenUnits() {
        Units.system = UnitSystem.METRIC
        assertEquals("50 m", Units.distance(48.0))
        assertEquals("1.4 km", Units.distance(1_440.0))
        Units.system = UnitSystem.IMPERIAL
        assertEquals("60 ft", Units.distance(17.07))
        assertEquals("1.5 mi", Units.distance(1.5 * M_PER_MILE))
    }

    @Test fun theUnitSettingWinsOverThePhonesRegion() {
        Locale.setDefault(Locale.US)
        Units.system = UnitSystem.METRIC
        assertEquals("1.4 km", Units.distance(1_440.0))
        Locale.setDefault(Locale.GERMANY)
        Units.system = UnitSystem.IMPERIAL
        assertEquals("25 mi²", Units.area(M_PER_MILE * M_PER_MILE * 25.0))
    }

    @Test fun areasAreWhole() {
        Units.system = UnitSystem.METRIC
        assertEquals("3 km²", Units.area(2_500_000.0))
        assertEquals("<1 km²", Units.area(200_000.0))
    }

    @Test fun aDistanceThatIsNotANumberDoesNotCrash() {
        Units.system = UnitSystem.METRIC
        assertEquals("–", Units.distance(Double.NaN))
    }

    @Test fun percentRoundsToAWholeNumber() {
        assertEquals("38%", Units.percent(0.375))
        assertEquals("0%", Units.percent(0.0))
        assertEquals("100%", Units.percent(1.0))
    }

    @Test fun percentNeverShowsMinusZero() = assertEquals("0%", Units.percent(-1e-9))

    @Test fun numbersUseAPointAndWesternDigitsInEveryLocale() {
        Units.system = UnitSystem.METRIC
        Locale.setDefault(Locale.forLanguageTag("ar-EG"))
        assertEquals("38%", Units.percent(0.375))
        assertEquals("1.4 km", Units.distance(1_440.0))
        Locale.setDefault(Locale.GERMANY)
        assertEquals("1.4 km", Units.distance(1_440.0))
    }
}
