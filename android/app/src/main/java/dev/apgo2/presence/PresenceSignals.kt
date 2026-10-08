package dev.apgo2.presence

data class WifiId(
    val ssid: String?,
    val bssid: String?,
)

data class HomeNetwork(
    val ssid: String,
    val bssid: String?,
)

data class CarDevice(
    val name: String,
    val address: String,
)

/** Turning what Android reports into the `Boolean?` signals the policy reads. Pure. */
object PresenceSignals {
    /** Android quotes SSIDs and reports `<unknown ssid>` when it may not tell. */
    fun cleanSsid(raw: String?): String? {
        val s = raw?.trim()?.removeSurrounding("\"") ?: return null
        return if (s.isEmpty() || s.equals("<unknown ssid>", ignoreCase = true)) null else s
    }

    /** The network with a cleaned SSID, or `null` when the name is not usable (for example no location permission). */
    fun usableNetwork(w: WifiId?): WifiId? {
        val ssid = cleanSsid(w?.ssid) ?: return null
        return WifiId(ssid, w?.bssid)
    }

    /** `true` at home, `false` on another network, `null` when unknown (not connected, name hidden, or no home network saved). */
    fun isHome(
        current: WifiId?,
        saved: List<HomeNetwork>,
    ): Boolean? {
        if (saved.isEmpty()) return null
        val ssid = cleanSsid(current?.ssid)
        val bssid = current?.bssid?.lowercase()?.takeIf { it.isNotBlank() && it != "02:00:00:00:00:00" }
        if (ssid == null && bssid == null) return null
        return saved.any { h -> (bssid != null && h.bssid?.lowercase() == bssid) || (ssid != null && h.ssid == ssid) }
    }

    fun carConnected(
        connectedAddresses: Set<String>?,
        saved: List<CarDevice>,
    ): Boolean? {
        if (saved.isEmpty() || connectedAddresses == null) return null
        val connected = connectedAddresses.map { it.lowercase() }.toSet()
        return saved.any { it.address.lowercase() in connected }
    }
}

/** Words for the Play screen's presence chip. Pure. */
object PresenceText {
    /** "Protection off" when no home network or car is saved, since nothing can pause the game then (the Home card offers the setup). */
    fun chip(
        state: PresenceState,
        configured: Boolean,
    ): String =
        if (!configured) {
            "Protection off"
        } else {
            when (state) {
                PresenceState.InZone -> "Tracking"
                PresenceState.AtHome -> "At home, paused"
                PresenceState.InCar -> "In car, not counting"
                PresenceState.OutsideZones -> "Outside zones, saving battery"
                PresenceState.Stopped -> "Not playing"
            }
        }
}
