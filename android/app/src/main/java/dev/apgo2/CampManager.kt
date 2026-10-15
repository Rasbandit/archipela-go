package dev.apgo2

import dev.apgo2.presence.GeoFix
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.CampIn

/** The player's camps on the app model: saves through the core, then reloads the list and tells presence. */
internal class CampManager(
    private val model: AppModel,
) {
    private val book =
        CampBook(
            object : CampStore {
                override fun camps() = model.engine.camps()

                override fun save(c: CampIn) = model.engine.saveCamp(c)

                override fun delete(id: String) = model.engine.deleteCamp(id)
            },
            object : CampWifi {
                override fun networkCount(campId: String) = model.settings.networkCount(campId)

                override fun forget(campId: String) = model.settings.removeCamp(campId)
            },
        )

    /** The name a new camp starts with. */
    fun defaultName() = book.defaultName()

    /** The list rows, with distances from where the phone is. */
    fun rows() = book.rows(model.here?.let { GeoFix(it.latitude, it.longitude, 0.0) })

    /** Creates ([id] null) or updates a camp and returns its id, or null (with a status message) when it could not be saved. */
    fun save(
        id: String?,
        name: String,
        at: LatLng,
    ): String? =
        changed("save") {
            if (id == null) book.add(name, at.latitude, at.longitude) else id.also { book.edit(it, name, at.latitude, at.longitude) }
        }

    /** Deletes a camp and the Wi-Fi networks saved for it. */
    fun delete(id: String) {
        changed("delete") { book.delete(id) }
    }

    // Runs [change], then reloads the camps and lets presence follow (the circles moved); a failure reaches the status line.
    private fun <T> changed(
        what: String,
        change: () -> T,
    ): T? {
        val done = runCatching(change)
        done.onFailure { model.fail("${what}_camp", "Could not $what the camp", it) }
        model.camps = model.engine.camps()
        model.presence.evaluate()
        return done.getOrNull()
    }
}
