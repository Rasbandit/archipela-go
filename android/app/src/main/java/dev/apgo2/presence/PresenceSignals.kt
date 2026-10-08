package dev.apgo2.presence

data class WifiId(val ssid: String?, val bssid: String?)
data class HomeNetwork(val ssid: String, val bssid: String?)
data class CarDevice(val name: String, val address: String)

/** Turning what Android reports into the `Boolean?` signals the policy reads. Pure. */
object PresenceSignals {
    /** Android quotes SSIDs and reports `<unknown ssid>` when it may not tell. */
    fun cleanSsid(raw: String?): String? {
        val s = raw?.trim()?.removeSurrounding("\"") ?: return null
        return if (s.isEmpty() || s.equals("<unknown ssid>", ignoreCase = true)) null else s
    }

    /** `true` at home, `false` on another network, `null` when unknown (not connected, name hidden, or no home network saved). */
    fun isHome(current: WifiId?, saved: List<HomeNetwork>): Boolean? {
        if (saved.isEmpty()) return null
        val ssid = cleanSsid(current?.ssid)
        val bssid = current?.bssid?.lowercase()?.takeIf { it.isNotBlank() && it != "02:00:00:00:00:00" }
        if (ssid == null && bssid == null) return null
        return saved.any { h -> (bssid != null && h.bssid?.lowercase() == bssid) || (ssid != null && h.ssid == ssid) }
    }

    fun carConnected(connectedAddresses: Set<String>?, saved: List<CarDevice>): Boolean? {
        if (saved.isEmpty() || connectedAddresses == null) return null
        val connected = connectedAddresses.map { it.lowercase() }.toSet()
        return saved.any { it.address.lowercase() in connected }
    }
}
