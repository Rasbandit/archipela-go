package dev.apgo2

import dev.apgo2.presence.GeoFix
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.CampIn

/** The player's camps on the app model: saves through the core, then reloads the list and tells presence. */
internal class CampManager(
    private val model: AppModel,
    private val scope: CoroutineScope,
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

    /** The list rows, from the camps in memory, with distances from where the phone is. */
    fun rows() = book.rows(model.camps, model.here?.let { GeoFix(it.latitude, it.longitude, 0.0) })

    /**
     * Creates ([id] null) or updates a camp, then hands [onSaved] its id, or null (with a status message) when it could not be
     * saved.
     */
    fun save(
        id: String?,
        name: String,
        at: LatLng,
        onSaved: (String?) -> Unit,
    ) = changed("save", onSaved) {
        when {
            id == null -> book.add(name, at.latitude, at.longitude)
            book.edit(id, name, at.latitude, at.longitude) -> id
            else -> error("No camp $id")
        }
    }

    /** Deletes a camp and the Wi-Fi networks saved for it. */
    fun delete(id: String) = changed("delete", {}) { book.delete(id) }

    // Runs [change] off the main thread (it writes camps.json and the settings), then back on it reloads the camps, lets presence
    // follow (the circles moved) and hands the result to [done]; a failure reaches the status line.
    private fun <T> changed(
        what: String,
        done: (T?) -> Unit,
        change: () -> T,
    ) {
        scope.launch {
            val r = withContext(Dispatchers.IO) { runCatching(change) }
            r.onFailure { model.fail("${what}_camp", "Could not $what the camp", it) }
            model.camps = model.engine.camps()
            model.presence.evaluate()
            done(r.getOrNull())
        }
    }
}
