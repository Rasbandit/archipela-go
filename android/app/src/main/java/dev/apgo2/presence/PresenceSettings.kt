package dev.apgo2.presence

import android.content.Context
import androidx.core.content.edit
import org.json.JSONArray
import org.json.JSONObject

/** App-level presence settings (platform identifiers, so they live in preferences, not in the core). */
internal class PresenceSettings(
    ctx: Context,
) {
    private val prefs = ctx.getSharedPreferences("presence", Context.MODE_PRIVATE)

    // Parsed lists are kept in memory (evaluation and the Play chip read them constantly); only this class writes the preferences.
    private var homeCache: List<HomeNetwork>? = null
    private var carCache: List<CarDevice>? = null

    var homeNetworks: List<HomeNetwork>
        get() = homeCache ?: parseHome().also { homeCache = it }
        private set(v) {
            val json = JSONArray(v.map { JSONObject().put("ssid", it.ssid).put("bssid", it.bssid ?: "") }).toString()
            prefs.edit { putString("home", json) }
            homeCache = v
        }

    var carDevices: List<CarDevice>
        get() = carCache ?: parseCar().also { carCache = it }
        private set(v) {
            val json = JSONArray(v.map { JSONObject().put("name", it.name).put("address", it.address) }).toString()
            prefs.edit { putString("car", json) }
            carCache = v
        }

    /** True once the player finished or explicitly skipped the setup wizard; until then the wizard opens on each app (process) start. */
    var setupDone: Boolean
        get() = prefs.getBoolean("setup_done", false)
        set(v) {
            prefs.edit { putBoolean("setup_done", v) }
        }

    /** Networks the player answered "Not this one" for in the home Wi-Fi offer (cleaned SSIDs); they are never offered again. */
    val mutedHomeOffers: Set<String>
        get() = prefs.getStringSet("muted_home_offers", null)?.toSet() ?: emptySet()

    /** [ssid] is the offer's already-cleaned name; it is stored as is. */
    fun muteHomeOffer(ssid: String) {
        prefs.edit { putStringSet("muted_home_offers", HomeWifiOffer.mute(mutedHomeOffers, ssid)) }
    }

    private fun parseHome(): List<HomeNetwork> =
        runCatching {
            val a = JSONArray(prefs.getString("home", "[]"))
            (0 until a.length()).mapNotNull { i ->
                runCatching {
                    a.getJSONObject(i).let { o ->
                        HomeNetwork(o.getString("ssid"), o.optString("bssid").takeIf { b -> b.isNotBlank() })
                    }
                }.getOrNull()
            }
        }.getOrDefault(emptyList())

    private fun parseCar(): List<CarDevice> =
        runCatching {
            val a = JSONArray(prefs.getString("car", "[]"))
            (0 until a.length()).mapNotNull { i ->
                runCatching { a.getJSONObject(i).let { o -> CarDevice(o.getString("name"), o.getString("address")) } }.getOrNull()
            }
        }.getOrDefault(emptyList())

    fun addHome(n: HomeNetwork) {
        val key = PresenceSignals.cleanSsid(n.ssid) ?: n.ssid
        homeNetworks = homeNetworks.filterNot { (PresenceSignals.cleanSsid(it.ssid) ?: it.ssid) == key } + n
    }

    fun removeHome(ssid: String) {
        val key = PresenceSignals.cleanSsid(ssid) ?: ssid
        homeNetworks = homeNetworks.filterNot { (PresenceSignals.cleanSsid(it.ssid) ?: it.ssid) == key }
    }

    fun setCar(devices: List<CarDevice>) {
        carDevices = devices
    }
}
