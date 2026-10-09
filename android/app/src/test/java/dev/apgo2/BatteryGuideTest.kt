package dev.apgo2

import dev.apgo2.ui.Help
import dev.apgo2.ui.SetupText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class BatteryGuideTest {
    @Test fun eachMakerGetsItsOwnGuidanceAndUnknownMakersTheGeneralOne() {
        assertEquals(Help.batterySamsung, BatteryGuide.forMaker("samsung"))
        assertEquals(Help.batteryXiaomi, BatteryGuide.forMaker("Xiaomi"))
        assertEquals(Help.batteryXiaomi, BatteryGuide.forMaker("Redmi"))
        assertEquals(Help.batteryXiaomi, BatteryGuide.forMaker("POCO"))
        assertEquals(Help.batteryHuawei, BatteryGuide.forMaker("HUAWEI"))
        assertEquals(Help.batteryHuawei, BatteryGuide.forMaker("honor"))
        assertEquals(Help.batteryOppo, BatteryGuide.forMaker("OnePlus"))
        assertEquals(Help.batteryOppo, BatteryGuide.forMaker("realme"))
        assertEquals(Help.batteryOppo, BatteryGuide.forMaker("OPPO"))
        assertEquals(Help.batteryOther, BatteryGuide.forMaker("Google"))
        assertEquals(Help.batteryOther, BatteryGuide.forMaker(""))
    }

    @Test fun theStepIsOnlyShownWhileBatteryOptimizationIsOn() {
        assertTrue(BatteryGuide.needed(ignoringOptimizations = false))
        assertFalse(BatteryGuide.needed(ignoringOptimizations = true))
    }

    @Test fun comingBackFromSettingsConfirmsOnlyOnceOptimizationIsOff() {
        assertEquals(SetupText.BATTERY_DONE, BatteryGuide.confirmation(ignoringOptimizations = true))
        assertNull(BatteryGuide.confirmation(ignoringOptimizations = false))
    }
}
