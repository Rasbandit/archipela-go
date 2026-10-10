package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.CircleOut
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.RealmOut
import java.util.UUID

private const val MIN_POLYGON_POINTS = 3

/** A realm that was just deleted and can still be brought back. */
internal data class UndoDelete(
    val id: String,
    val name: String,
)

/** Saving, marking and deleting realms, and the player's home point. */
internal class RealmManager(
    private val model: AppModel,
) {
    var undo by mutableStateOf<UndoDelete?>(null)
    private val hidden = mutableStateListOf<String>()

    /** The realms to show: those waiting out their undo window are hidden but not yet deleted. */
    val shown: List<RealmOut> get() = model.realms.filter { it.id !in hidden }

    /** Home, or failing that the first point of the first realm; null when there is neither. */
    fun homePoint(): GeoPoint? =
        model.engine.home() ?: model.realms.firstOrNull()?.let { r -> if (r.polygonActive) r.polygon.firstOrNull() else r.circle?.center }

    /** "Realm N" with the lowest N not already used by a realm. */
    fun defaultName(): String {
        val taken = model.realms.map { it.name }.toSet()
        return generateSequence(1) { it + 1 }.map { "Realm $it" }.first { it !in taken }
    }

    /**
     * Creates (id == null) or updates a realm and returns its id, or null (with a status message) when it could not be saved.
     * Both outlines are kept; [polygonActive] picks the real one. A blank name becomes "Realm N". Nothing is scanned here: that is
     * the caller's decision.
     */
    fun save(
        id: String?,
        name: String,
        icon: String?,
        circle: Pair<LatLng, Double>?,
        polygon: List<LatLng>,
        polygonActive: Boolean,
    ): String? {
        val outlineMissing = if (polygonActive) polygon.size < MIN_POLYGON_POINTS else circle == null
        if (outlineMissing) return null
        val rid = id ?: UUID.randomUUID().toString()
        val c = circle?.let { (p, r) -> CircleOut(GeoPoint(p.latitude, p.longitude), r) }
        val saved =
            runCatching {
                model.engine.saveRealm(
                    rid,
                    name.ifBlank { defaultName() },
                    icon,
                    c,
                    polygon.map { GeoPoint(it.latitude, it.longitude) },
                    polygonActive,
                )
            }
        saved.onSuccess {
            model.realms = model.engine.realms()
            model.library.refreshStreets(rid) // an open game playing in this realm rebuilds its streets for the new outline
        }
        saved.onFailure { model.fail("save", "Could not save", it) }
        return rid.takeIf { saved.isSuccess }
    }

    /** Favorite or ban a scanned place ("none" clears it). Offers update now; quests change the next time they are made or re-rolled. */
    fun setFindMark(
        realmId: String,
        placeId: String,
        mark: String,
    ): Boolean {
        val marked = runCatching { model.engine.setFindMark(realmId, placeId, mark) }
        marked.onSuccess { model.offers[realmId] = model.engine.realmOffers(realmId) }
        marked.onFailure { model.fail("save", "Could not save", it) }
        return marked.isSuccess
    }

    /** Hide a realm and offer Undo; it is really deleted when [commitDelete] runs (after the undo bar goes away). */
    fun deleteWithUndo(id: String) {
        val r = model.realms.firstOrNull { it.id == id } ?: return
        undo?.let { commitDelete(it.id) } // a new delete settles the previous one
        hidden.add(id)
        undo = UndoDelete(id, r.name)
    }

    /** Bring a hidden realm back. */
    fun undoDelete(id: String) {
        hidden.remove(id)
        if (undo?.id == id) undo = null
    }

    /** Really delete a realm that was hidden by [deleteWithUndo]. */
    fun commitDelete(id: String) {
        if (id !in hidden) return
        runCatching { model.engine.deleteRealm(id) }
        hidden.remove(id)
        if (undo?.id == id) undo = null
        model.refreshAll()
    }

    /** Make the current position home. */
    fun setHomeHere() {
        val c = model.here
        if (c == null) {
            model.status = "No location yet"
            return
        }
        setHome(c)
    }

    /** Saves home. [announce] shows "Home set" in the global status line; screens that show their own confirmation pass false. */
    fun setHome(
        p: LatLng,
        announce: Boolean = true,
    ) {
        val saved = runCatching { model.engine.setHome(GeoPoint(p.latitude, p.longitude)) }
        saved.onSuccess {
            model.home = model.engine.home()
            if (announce) model.status = "Home set"
        }
        saved.onFailure { model.fail("set_home", "Could not set home", it) }
    }
}
