package dev.apgo2

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.FindOut

private const val NO_MARK = "none"
private const val DEFAULT_BUBBLE_DP = 230
private const val BUBBLE_GAP_DP = 26

// What the player can do in the realm editor with the finds and with the shape, as seen from the screens.

/** Load what the scan found, once there is a scan. */
internal suspend fun RealmEditorState.loadFinds() {
    val rid = id
    if (rid != null && current?.scannedAtMs != null) {
        val all = withContext(Dispatchers.IO) { m.engine.realmFinds(rid) }
        finds.clear()
        finds.addAll(all)
        findsVersion++
    }
}

/** Mark a find [to] favorite or banned; marking it the same way again clears the mark. */
internal fun RealmEditorState.mark(
    f: FindOut,
    to: String,
) {
    val next = if (f.mark == to) NO_MARK else to // tapping a lit toggle clears it
    val rid = id ?: return
    if (m.realmOps.setFindMark(rid, f.id, next)) {
        finds[finds.indexOfFirst { it.id == f.id }] = f.copy(mark = next)
        findsVersion++
    }
}

/** Select a find and bring it into view together with its callout. The callout's real height is measured once it is shown. */
internal fun RealmEditorState.show(
    f: FindOut,
    density: Float,
) {
    selectedFind = f.id
    focus = MapFocus(LatLng(f.at.lat, f.at.lon), nextFocusNonce(), roomAbove(density))
}

/** The callout has been measured: bring the selected find into view again with its real height. */
internal fun RealmEditorState.refocusOnBubble(density: Float) {
    val f = finds.firstOrNull { it.id == selectedFind } ?: return
    if (bubblePx > 0) focus = MapFocus(LatLng(f.at.lat, f.at.lon), nextFocusNonce(), roomAbove(density))
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

// Each request to the map carries a new number so the same point can be asked for twice.
private fun RealmEditorState.nextFocusNonce() = (focus?.nonce ?: 0) + 1

private fun RealmEditorState.roomAbove(density: Float) =
    (if (bubblePx > 0) bubblePx else (DEFAULT_BUBBLE_DP * density).toInt()) + (BUBBLE_GAP_DP * density).toInt()
