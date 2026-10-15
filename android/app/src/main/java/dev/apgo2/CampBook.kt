package dev.apgo2

import dev.apgo2.presence.GeoFix
import dev.apgo2.ui.distanceM
import uniffi.apgo_ffi.CampIn
import uniffi.apgo_ffi.CampOut

/** The saved camps, as the core keeps them. */
internal interface CampStore {
    fun camps(): List<CampOut>

    /** Saves [c] and returns its id; the core names a camp saved with a blank name. */
    fun save(c: CampIn): String

    fun delete(id: String)
}

/** The Wi-Fi networks saved per camp (a platform setting, so it lives outside the core). */
internal interface CampWifi {
    fun networkCount(campId: String): Int

    fun forget(campId: String)
}

/** One line of the camps list. [distanceM] is null while the phone's position is unknown. */
internal data class CampRow(
    val id: String,
    val name: String,
    val lat: Double,
    val lon: Double,
    val distanceM: Double?,
    val hasWifi: Boolean,
)

/** Adding, renaming, moving and deleting camps, and the list the camps screen shows. Pure over [CampStore] and [CampWifi]. */
internal class CampBook(
    private val store: CampStore,
    private val wifi: CampWifi,
) {
    /** Saves a new camp (a blank [name] gets the core's default) and returns its id. */
    fun add(
        name: String,
        lat: Double,
        lon: Double,
    ): String = store.save(CampIn("", name.trim(), lat, lon))

    /** Renames and/or moves camp [id]; the core keeps the old name for a blank [name]. False (nothing saved) when there is no such camp. */
    fun edit(
        id: String,
        name: String,
        lat: Double,
        lon: Double,
    ): Boolean {
        if (store.camps().none { it.id == id }) return false
        store.save(CampIn(id, name.trim(), lat, lon))
        return true
    }

    /** Deletes camp [id] and forgets the Wi-Fi networks saved for it. */
    fun delete(id: String) {
        deleteCamp(id)
        forgetWifi(id)
    }

    /** The core half of [delete] (writes camps.json: call it off the main thread). */
    fun deleteCamp(id: String) = store.delete(id)

    /** The settings half of [delete] (Compose state: call it on the main thread). */
    fun forgetWifi(id: String) = wifi.forget(id)

    /** The list rows for [camps] (in memory, so cheap enough to call while drawing), with the distance from [here] when known. */
    fun rows(
        camps: List<CampOut>,
        here: GeoFix?,
    ): List<CampRow> =
        camps.map {
            val distance = here?.let { h -> distanceM(h, GeoFix(it.lat, it.lon, 0.0)) }
            CampRow(it.id, it.name, it.lat, it.lon, distance, wifi.networkCount(it.id) > 0)
        }
}
