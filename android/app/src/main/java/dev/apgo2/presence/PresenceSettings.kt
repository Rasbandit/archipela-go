package dev.apgo2.presence

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/** App-level presence settings (platform identifiers, so they live in preferences, not in the core). */
class PresenceSettings(ctx: Context) {
    private val prefs = ctx.getSharedPreferences("presence", Context.MODE_PRIVATE)

    var homeNetworks: List<HomeNetwork>
        get() = runCatching {
            val a = JSONArray(prefs.getString("home", "[]"))
            (0 until a.length()).map { a.getJSONObject(it).let { o -> HomeNetwork(o.getString("ssid"), o.optString("bssid").takeIf { b -> b.isNotBlank() }) } }
        }.getOrDefault(emptyList())
        private set(v) = prefs.edit().putString("home", JSONArray(v.map { JSONObject().put("ssid", it.ssid).put("bssid", it.bssid ?: "") }).toString()).apply()

    var carDevices: List<CarDevice>
        get() = runCatching {
            val a = JSONArray(prefs.getString("car", "[]"))
            (0 until a.length()).map { a.getJSONObject(it).let { o -> CarDevice(o.getString("name"), o.getString("address")) } }
        }.getOrDefault(emptyList())
        private set(v) = prefs.edit().putString("car", JSONArray(v.map { JSONObject().put("name", it.name).put("address", it.address) }).toString()).apply()

    fun addHome(n: HomeNetwork) { homeNetworks = homeNetworks.filterNot { it.ssid == n.ssid } + n }
    fun removeHome(ssid: String) { homeNetworks = homeNetworks.filterNot { it.ssid == ssid } }
    fun setCar(devices: List<CarDevice>) { carDevices = devices }
}
