package dev.apgo2.presence

/**
 * One row in the home Wi-Fi list. [saved] means it is ticked (a home network); [bssid] is only known for the connected or an
 * already saved one.
 */
data class WifiChoice(
    val ssid: String,
    val bssid: String?,
    val saved: Boolean,
    val connected: Boolean,
)

/** What the setup wizard lists for home Wi-Fi. Pure. */
object WifiChoices {
    /**
     * Connected network first, then saved, then nearby A-Z; one row per name; unusable names dropped; [query] filters by name,
     * ignoring case.
     */
    fun merge(
        saved: List<HomeNetwork>,
        current: WifiId?,
        nearby: List<String>,
        query: String,
    ): List<WifiChoice> {
        val savedBySsid = saved.mapNotNull { h -> PresenceSignals.cleanSsid(h.ssid)?.let { it to h } }.toMap()
        val now = PresenceSignals.cleanSsid(current?.ssid)
        val ordered = LinkedHashSet<String>()
        now?.let { ordered += it }
        ordered += savedBySsid.keys
        ordered += nearby.mapNotNull { PresenceSignals.cleanSsid(it) }.distinct().sortedWith(String.CASE_INSENSITIVE_ORDER)
        val q = query.trim()
        return ordered.filter { q.isEmpty() || it.contains(q, ignoreCase = true) }.map { ssid ->
            WifiChoice(ssid, if (ssid == now) current?.bssid else savedBySsid[ssid]?.bssid, ssid in savedBySsid, ssid == now)
        }
    }
}

/** What the setup wizard lists for the car. Pure. */
object CarChoices {
    /** Paired devices first, then saved ones that are no longer paired (so they can still be removed), filtered by name. */
    fun merge(
        paired: List<CarDevice>,
        saved: List<CarDevice>,
        query: String,
    ): List<CarDevice> {
        val all = paired + saved.filterNot { s -> paired.any { it.address.equals(s.address, ignoreCase = true) } }
        val q = query.trim()
        return all.filter { q.isEmpty() || it.name.contains(q, ignoreCase = true) }
    }
}
