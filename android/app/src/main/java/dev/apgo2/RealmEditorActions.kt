package dev.apgo2

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.ui.BubblePlacement
import dev.apgo2.ui.MapMarkers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.FindOut

private const val NO_MARK = "none"
private const val DEFAULT_BUBBLE_DP = 230

// What the player can do in the realm editor with the finds and with the shape, as seen from the screens.

/**
 * The finds the scan found for the realm being edited, the selected one, and where the map should look. [fetch] reads a realm's
 * finds (blocking); [saveMark] stores a find's mark and tells whether that worked.
 */
@Stable
internal class EditorFinds(
    private val fetch: (realmId: String) -> List<FindOut>,
    private val saveMark: (realmId: String, findId: String, mark: String) -> Boolean,
) {
    private val list = mutableStateListOf<FindOut>()

    /** Every find of the realm, in scan order. */
    val all: List<FindOut> get() = list

    /** Bumped on every change to [all], so views can key on it. */
    var version by mutableIntStateOf(0)
        private set

    /** The id of the selected find, if any. */
    var selected by mutableStateOf<String?>(null)

    /** The last request to bring a point into view. */
    var focus by mutableStateOf<MapFocus?>(null)
        private set

    /** The measured height of the selected find's callout; 0 until it is shown. */
    var bubblePx by mutableIntStateOf(0)

    /** Replace the finds with what the scan of realm [rid] found. */
    suspend fun load(rid: String) {
        val found = withContext(Dispatchers.IO) { fetch(rid) }
        list.clear()
        list.addAll(found)
        version++
    }

    /** Mark a find of realm [rid] [to] favorite or banned; marking it the same way again clears the mark. */
    fun mark(
        rid: String,
        f: FindOut,
        to: String,
    ) {
        val next = if (f.mark == to) NO_MARK else to // tapping a lit toggle clears it
        if (saveMark(rid, f.id, next)) {
            list[list.indexOfFirst { it.id == f.id }] = f.copy(mark = next)
            version++
        }
    }

    /** Select a find and bring it into view together with its callout. The callout's real height is measured once it is shown. */
    fun show(
        f: FindOut,
        density: Float,
    ) {
        selected = f.id
        focus = MapFocus(LatLng(f.at.lat, f.at.lon), nextFocusNonce(), roomAbove(density))
    }

    /** The callout has been measured: bring the selected find into view again with its real height. */
    fun refocusOnBubble(density: Float) {
        val f = list.firstOrNull { it.id == selected } ?: return
        if (bubblePx > 0) focus = MapFocus(LatLng(f.at.lat, f.at.lon), nextFocusNonce(), roomAbove(density))
    }

    // Each request to the map carries a new number so the same point can be asked for twice.
    private fun nextFocusNonce() = (focus?.nonce ?: 0) + 1

    private fun roomAbove(density: Float) =
        (if (bubblePx > 0) bubblePx else (DEFAULT_BUBBLE_DP * density).toInt()) +
            BubblePlacement.gapPx(MapMarkers.selectedFindPinHeightPx(), density)
}

/** Load what the scan found, once there is a scan. */
internal suspend fun RealmEditorState.loadFinds() {
    val rid = id
    if (rid != null && current?.scannedAtMs != null) finds.load(rid)
}

/** Mark a find [to] favorite or banned; marking it the same way again clears the mark. */
internal fun RealmEditorState.mark(
    f: FindOut,
    to: String,
) {
    finds.mark(id ?: return, f, to)
}

/** The points the player can grab: a circle has a handle at its center (moves it) and its whole ring is an invisible one. */
internal fun RealmEditorState.handles(): List<LatLng> =
    when {
        tab != EditorTab.AREA -> emptyList()
        polygon -> m.draft.toList()
        else -> listOfNotNull(circleCenter)
    }

/** A tap on the map adds a polygon corner. */
internal fun RealmEditorState.onMapTap(at: LatLng) {
    if (polygon && tab == EditorTab.AREA) {
        m.draft.add(at)
        commit()
    }
}

/** Remove every polygon corner. */
internal fun RealmEditorState.clearCorners() {
    m.draft.clear()
    commit()
}
