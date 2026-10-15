package dev.apgo2

import dev.apgo2.presence.GeoFix
import dev.apgo2.presence.HomeWifiOffer
import uniffi.apgo_ffi.CampIn
import uniffi.apgo_ffi.CampOut

/** The saved camps, as the core keeps them. */
internal interface CampStore {
    fun camps(): List<CampOut>

    fun save(c: CampIn)

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

private const val FIRST_CAMP_NAME = "Home"

/** Adding, renaming, moving and deleting camps, and the list the camps screen shows. Pure over [CampStore] and [CampWifi]. */
internal class CampBook(
    private val store: CampStore,
    private val wifi: CampWifi,
) {
    /** "Home" for the first camp, else "Camp N" with the lowest N not taken (counting from the number of camps plus one). */
    fun defaultName(): String {
        val camps = store.camps()
        if (camps.isEmpty()) return FIRST_CAMP_NAME
        val taken = camps.map { it.name }.toSet()
        return generateSequence(camps.size + 1) { it + 1 }.map { "Camp $it" }.first { it !in taken }
    }

    /** Saves a new camp (a blank [name] takes [defaultName]) and returns its id. */
    fun add(
        name: String,
        lat: Double,
        lon: Double,
    ): String {
        val label = name.trim().ifEmpty { defaultName() }
        val before = store.camps().map { it.id }.toSet()
        store.save(CampIn("", label, lat, lon))
        return store.camps().first { it.id !in before }.id
    }

    /** Renames and/or moves camp [id]; a blank [name] keeps the old name. False (nothing saved) when there is no such camp. */
    fun edit(
        id: String,
        name: String,
        lat: Double,
        lon: Double,
    ): Boolean {
        val old = store.camps().firstOrNull { it.id == id } ?: return false
        store.save(CampIn(id, name.trim().ifEmpty { old.name }, lat, lon))
        return true
    }

    /** Deletes camp [id] and forgets the Wi-Fi networks saved for it. */
    fun delete(id: String) {
        store.delete(id)
        wifi.forget(id)
    }

    /** The list rows for [camps] (in memory, so cheap enough to call while drawing), with the distance from [here] when known. */
    fun rows(
        camps: List<CampOut>,
        here: GeoFix?,
    ): List<CampRow> =
        camps.map {
            val distance = here?.let { h -> HomeWifiOffer.distanceM(h, GeoFix(it.lat, it.lon, 0.0)) }
            CampRow(it.id, it.name, it.lat, it.lon, distance, wifi.networkCount(it.id) > 0)
        }
}
