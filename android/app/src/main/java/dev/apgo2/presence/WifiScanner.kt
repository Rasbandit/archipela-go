package dev.apgo2.presence

import android.annotation.SuppressLint
import android.content.Context
import android.net.wifi.WifiManager

/** Reads the Wi-Fi networks in range, to offer as home networks. Needs the location permission the app already asks for. */
class WifiScanner(
    ctx: Context,
) {
    private val wifi = ctx.applicationContext.getSystemService(WifiManager::class.java)

    /** Names from the last scan, raw (blank for hidden networks); empty without permission or with Wi-Fi off. Feed to [WifiChoices.merge]. */
    @Suppress("DEPRECATION")
    @SuppressLint("MissingPermission")
    fun nearby(): List<String> = runCatching { wifi?.scanResults?.map { it.SSID } ?: emptyList() }.getOrDefault(emptyList())

    /** Ask for a fresh scan; false when Android refused (throttled to about 4 per 2 minutes, Wi-Fi off, no permission). Results arrive later via [nearby]. */
    @Suppress("DEPRECATION")
    @SuppressLint("MissingPermission")
    fun rescan(): Boolean = runCatching { wifi?.startScan() == true }.getOrDefault(false)
}
