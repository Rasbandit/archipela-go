package dev.apgo2

import android.content.Context
import android.os.PowerManager
import dev.apgo2.ui.Help
import dev.apgo2.ui.HelpTopic
import dev.apgo2.ui.SetupText

/** Which battery guidance a phone gets, by `Build.MANUFACTURER`. */
internal object BatteryGuide {
    fun forMaker(manufacturer: String): HelpTopic =
        when (manufacturer.trim().lowercase()) {
            "samsung" -> Help.batterySamsung
            "xiaomi", "redmi", "poco" -> Help.batteryXiaomi
            "huawei", "honor" -> Help.batteryHuawei
            "oneplus", "oppo", "realme" -> Help.batteryOppo
            else -> Help.batteryOther
        }

    /** What the step says after the player returns from settings: a confirmation once optimisation is off, else nothing. */
    fun confirmation(ignoringOptimizations: Boolean): String? = if (ignoringOptimizations) SetupText.BATTERY_DONE else null

    /** The step is shown only while the app is still battery-optimised. */
    fun needed(ignoringOptimizations: Boolean): Boolean = !ignoringOptimizations
}

/** Whether the app is exempt from battery optimisation (the setup step reads it). */
internal fun Context.ignoringBatteryOptimizations(): Boolean =
    getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(packageName)
