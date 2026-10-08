package dev.apgo2.presence

enum class SetupStep { Home, Wifi, Car }

/** What the player has set up so far. Pure: the Home card and the wizard both read it. */
data class SetupProgress(val homeSet: Boolean, val wifiCount: Int, val carCount: Int, val setupDone: Boolean) {
    /** Home is set but no home Wi-Fi is saved, so nothing can pause the game at home. */
    val missingWifi: Boolean get() = homeSet && wifiCount == 0

    /** First step that still has nothing in it, or null when all three have something. */
    fun nextStep(): SetupStep? = when {
        !homeSet -> SetupStep.Home
        wifiCount == 0 -> SetupStep.Wifi
        carCount == 0 -> SetupStep.Car
        else -> null
    }

    /** Show the "Finish setup" nag: the wizard was never finished, or home Wi-Fi is missing. A skipped car step is fine. */
    fun needsAttention(): Boolean = !setupDone || missingWifi
}
