package dev.apgo2.presence

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.edit
import dev.apgo2.Diag
import org.json.JSONArray
import org.json.JSONObject

private const val TAG = "presence-settings"

/** App-level presence settings (platform identifiers, so they live in preferences, not in the core). */
internal class PresenceSettings(
    ctx: Context,
) {
    private val prefs = ctx.getSharedPreferences("presence", Context.MODE_PRIVATE)

    // Parsed lists are kept in memory (evaluation and the Play chip read them constantly); only this class writes the preferences.
    // Compose state, so a screen listing the saved networks redraws when one is added or removed.
    private var homeCache by mutableStateOf<List<HomeNetwork>?>(null)
    private var carCache: List<CarDevice>? = null

    var homeNetworks: List<HomeNetwork>
        get() = homeCache ?: parseHome().also { homeCache = it }
        private set(v) {
            val json =
                JSONArray(
                    v.map { JSONObject().put("ssid", it.ssid).put("bssid", it.bssid ?: "").put("campId", it.campId) },
                ).toString()
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
                        HomeNetwork(
                            o.getString("ssid"),
                            o.optString("bssid").takeIf { b -> b.isNotBlank() },
                            o.optString("campId").ifBlank { "home" },
                        )
                    }
                }.getOrNull()
            }
        }.getOrElse {
            Diag.warn(TAG, "saved home networks unreadable; none are used", "reason" to it.toString())
            emptyList()
        }

    private fun parseCar(): List<CarDevice> =
        runCatching {
            val a = JSONArray(prefs.getString("car", "[]"))
            (0 until a.length()).mapNotNull { i ->
                runCatching { a.getJSONObject(i).let { o -> CarDevice(o.getString("name"), o.getString("address")) } }.getOrNull()
            }
        }.getOrDefault(emptyList())

    /** Save [n] for its camp, replacing the same network saved for that camp; one network may serve several camps. */
    fun addHome(n: HomeNetwork) {
        homeNetworks = homeNetworks.filterNot { sameNetwork(it, n.ssid, n.campId) } + n
    }

    /** Forget network [ssid] for camp [campId] only; other camps keep it. */
    fun removeHome(
        ssid: String,
        campId: String,
    ) {
        homeNetworks = homeNetworks.filterNot { sameNetwork(it, ssid, campId) }
    }

    /** Tick ([on]) or untick network [ssid] for camp [campId] (the Wi-Fi setup step). */
    fun setHome(
        ssid: String,
        bssid: String?,
        campId: String,
        on: Boolean,
    ) = if (on) addHome(HomeNetwork(ssid, bssid, campId)) else removeHome(ssid, campId)

    // A saved network is keyed by its cleaned name and its camp.
    private fun sameNetwork(
        n: HomeNetwork,
        ssid: String,
        campId: String,
    ) = n.campId == campId && (PresenceSignals.cleanSsid(n.ssid) ?: n.ssid) == (PresenceSignals.cleanSsid(ssid) ?: ssid)

    /** How many saved networks pause camp [campId]. */
    fun networkCount(campId: String): Int = homeNetworks.count { it.campId == campId }

    /** Forget every network saved for camp [campId] (the camp was deleted). */
    fun removeCamp(campId: String) {
        homeNetworks = homeNetworks.filterNot { it.campId == campId }
    }

    fun setCar(devices: List<CarDevice>) {
        carDevices = devices
    }
}
