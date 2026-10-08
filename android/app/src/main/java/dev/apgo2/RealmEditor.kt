package dev.apgo2

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Button
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.History
import dev.apgo2.ui.IconLabel
import dev.apgo2.ui.MIN_POLYGON_CORNERS
import dev.apgo2.ui.SavedBadge
import dev.apgo2.ui.ToolButton
import dev.apgo2.ui.ToolPill
import dev.apgo2.ui.ToolPillRow
import dev.apgo2.ui.circleExtremes
import dev.apgo2.ui.insideShape
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.FindOut
import uniffi.apgo_ffi.RealmOut

/** The two tabs of the realm editor. */
internal object EditorTab {
    const val AREA = 0
    const val DETAILS = 1
}

/** The marks a find can carry, and the filter that shows all of them. */
internal object FindFilter {
    const val ALL = "all"
    const val FAVORITE = "favorite"
    const val BANNED = "banned"

    /** Whether [f] passes the mark [filter] and the search [query] (its name or a quest kind's name, ignoring case). */
    fun matches(
        f: FindOut,
        filter: String,
        query: String,
    ): Boolean =
        (filter == ALL || f.mark == filter) &&
            (query.isBlank() || f.name.contains(query, true) || f.kinds.any { it.name.contains(query, true) })
}

private const val DEFAULT_RADIUS_M = 1500f
private const val MIN_RADIUS_M = 300f
private const val MAX_RADIUS_M = 8000f
private const val NAME_SAVE_DELAY_MS = 600L
private const val OVERLAY_TOP_DP = 16

/** Everything the editor can change, so a step of Undo/Redo can restore it whole. */
private data class EditSnap(
    val polygon: Boolean,
    val radius: Float,
    val center: LatLng?,
    val corners: List<LatLng>,
    val name: String,
    val icon: String?,
)

// What the realm editor is showing and doing. A new realm is created by its first edit and named "Realm N"; every finished edit is
// saved at once, so there is no Save or Cancel: Undo goes back.
@Stable
internal class RealmEditorState(
    val m: AppModel,
    realmId: String?,
    val onClose: () -> Unit,
) {
    private val original = m.realms.firstOrNull { it.id == realmId }
    var id by mutableStateOf(realmId) // set when a new realm is first saved
    var tab by mutableIntStateOf(EditorTab.AREA)
    var name by mutableStateOf(original?.name ?: "")
    var icon by mutableStateOf(original?.icon)
    var pickingIcon by mutableStateOf(false)
    var confirmDelete by mutableStateOf<RealmOut?>(null)
    var polygon by mutableStateOf(original?.polygonActive == true)
        private set
    var radius by mutableFloatStateOf(original?.circle?.radiusM?.toFloat() ?: DEFAULT_RADIUS_M)
        private set
    var center by mutableStateOf(original?.circle?.let { LatLng(it.center.lat, it.center.lon) })
        private set
    var savedAt by mutableStateOf<Long?>(null) // when the realm was last written to disk
    val finds = EditorFinds({ m.engine.realmFinds(it) }, m.realmOps::setFindMark)
    var anchor by mutableStateOf<Offset?>(null)
    var query by mutableStateOf("")
    var filter by mutableStateOf(FindFilter.ALL)
    var panelPx by mutableIntStateOf(0)
    var fit by mutableStateOf<MapFit?>(null)
    private var deleted = false
    private var fitNonce = 0

    // The outline the finds were last fetched for. The shape counts as changed only while it differs from that one, so undoing back
    // to it is not a change.
    private var scannedKey by mutableStateOf(original?.takeIf { it.scannedAtMs != null }?.let(::outlineOf))

    init {
        m.draft.clear()
        original?.polygon?.forEach { m.draft.add(LatLng(it.lat, it.lon)) }
    }

    private val history = History(snapshot())

    val current: RealmOut? get() = m.realms.firstOrNull { it.id == id }

    // A new circle follows your GPS until it is edited.
    val circleCenter: LatLng? get() = center ?: m.me

    val shapeDirty: Boolean get() = outlineKey() != scannedKey

    val canUndo: Boolean get() = history.canUndo

    val canRedo: Boolean get() = history.canRedo

    fun undo() = history.undo()?.let(::apply)

    fun redo() = history.redo()?.let(::apply)

    // A finished edit: freeze a following circle in place, save, and record it for Undo.
    fun commit() {
        if (center == null && !polygon) center = m.me
        if (persist()) history.push(snapshot())
    }

    // Typing a name is saved once you pause.
    suspend fun saveNameAfterPause() {
        val unchanged = name == history.current.name && id != null
        val empty = name.isBlank() && id == null
        if (unchanged || empty) return
        delay(NAME_SAVE_DELAY_MS)
        commit()
    }

    // However the editor is left (X, Back, another tab): keep a pending name, and fetch finds for an outline that changed.
    fun leave() {
        if (!deleted) {
            if (name != history.current.name) commit()
            val rid = id
            if (rid != null && outlineKey() != scannedKey) m.scans.scan(rid)
        }
        m.draft.clear()
    }

    fun delete(realm: RealmOut) {
        confirmDelete = null
        deleted = true
        m.realmOps.deleteWithUndo(realm.id)
        onClose()
    }

    fun goTab(to: Int) {
        if (to == tab) return
        if (to == EditorTab.DETAILS) {
            openDetails()
        } else {
            // Back to the area: after the map settles, fit the whole shape in the view.
            fit = MapFit(shapePoints(), ++fitNonce)
            finds.selected = null
        }
        tab = to
    }

    fun pickIcon(key: String) {
        icon = key
        commit()
    }

    // A handle was dragged to [to]: a polygon corner moves, a circle moves (handle 0) or is resized (the ring).
    fun moveHandle(
        i: Int,
        to: LatLng,
    ) {
        if (polygon) {
            if (i in m.draft.indices) m.draft[i] = to
        } else {
            moveCircle(i, to)
        }
    }

    // Switch to a circle or a polygon, going back to the area tab.
    fun chooseShape(polygonShape: Boolean) {
        val changed = polygon != polygonShape
        polygon = polygonShape
        goTab(EditorTab.AREA)
        if (changed) commit()
    }

    private fun moveCircle(
        i: Int,
        to: LatLng,
    ) {
        val c = circleCenter ?: return
        if (i == 0) {
            center = to
        } else {
            val d = floatArrayOf(0f)
            android.location.Location.distanceBetween(c.latitude, c.longitude, to.latitude, to.longitude, d)
            radius = d[0].coerceIn(MIN_RADIUS_M, MAX_RADIUS_M)
        }
    }

    // Save what is on screen. Returns false when there is nothing to save yet (no location, or a polygon is not drawn).
    private fun persist(): Boolean {
        if (deleted) return false
        val c = center ?: m.me // read now: the value captured when the screen was last drawn may be older than this edit
        val rid =
            m.realmOps.save(
                id,
                name,
                icon,
                c?.let { it to radius.toDouble() },
                m.draft.toList(),
                polygon && m.draft.size >= MIN_POLYGON_CORNERS,
            )
        if (rid != null) {
            if (id == null) {
                id = rid
                name = m.realms.firstOrNull { it.id == rid }?.name ?: name // the default "Realm N"
            }
            savedAt = System.currentTimeMillis()
        }
        return rid != null
    }

    private fun apply(s: EditSnap) {
        polygon = s.polygon
        radius = s.radius
        center = s.center
        name = s.name.ifBlank { name } // the first state has no name yet: keep the default
        icon = s.icon
        m.draft.clear()
        m.draft.addAll(s.corners)
        persist()
    }

    private fun snapshot() = EditSnap(polygon, radius, center, m.draft.toList(), name, icon)

    private fun outlineKey() = EditSnap(polygon, radius, center, m.draft.toList(), "", null)

    private fun outlineOf(r: RealmOut) =
        EditSnap(
            r.polygonActive,
            r.circle?.radiusM?.toFloat() ?: DEFAULT_RADIUS_M,
            r.circle?.let { LatLng(it.center.lat, it.center.lon) },
            r.polygon.map { LatLng(it.lat, it.lon) },
            "",
            null,
        )

    private fun shapePoints(): List<LatLng> {
        val c = circleCenter
        return when {
            polygon -> m.draft.toList()
            c == null -> emptyList()
            else -> circleExtremes(c, radius.toDouble())
        }
    }

    // Opening Details creates a new realm if need be, and fetches finds for an outline that is new or changed.
    private fun openDetails() {
        if (id == null) commit()
        val rid = id
        if (rid != null && (outlineKey() != scannedKey || current?.scannedAtMs == null)) {
            m.scans.scan(rid)
            scannedKey = outlineKey()
        }
    }
}

// The realm editor. The map is the whole screen. A toolbar on the left switches between editing the area (Circle or Polygon, one or
// the other) and Details (name, icon and the finds the scan found); Undo, Redo and a close button sit on the right. [realmId] null
// starts a new realm.
@Composable
internal fun RealmEditor(
    m: AppModel,
    realmId: String?,
    onClose: () -> Unit,
) {
    val s = remember(realmId) { RealmEditorState(m, realmId, onClose) }
    val density = LocalDensity.current.density
    val view = rememberFindsView(s)
    val listState = rememberLazyListState()
    EditorEffects(s, view, listState, density)
    Box(Modifier.fillMaxSize()) {
        EditorMap(s, view, density)
        IconPickerDialog(s)
        ConfirmDelete(s.confirmDelete, onConfirm = s::delete, onDismiss = { s.confirmDelete = null })
        view.visible.firstOrNull { it.id == s.finds.selected }?.let { f ->
            s.anchor?.let { at ->
                FindBubble(f, at, onSize = { s.finds.bubblePx = it.height }, onMark = { s.mark(f, it) }, onClose = {
                    s.finds.selected =
                        null
                })
            }
        }
        ModeTools(s)
        HistoryTools(s)
        s.savedAt?.let { SavedBadge("All changes saved", Modifier.align(Alignment.TopEnd).padding(top = 72.dp, end = 16.dp)) }
        if (s.tab == EditorTab.AREA) AreaPanel(s) else DetailsPanel(s, view, listState)
    }
}

// What keeps the editor going: saving a typed name, saving on the way out, loading finds and keeping the selection in view.
@Composable
private fun EditorEffects(
    s: RealmEditorState,
    view: FindsView,
    listState: androidx.compose.foundation.lazy.LazyListState,
    density: Float,
) {
    LaunchedEffect(s.name) { s.saveNameAfterPause() }
    DisposableEffect(s) { onDispose { s.leave() } }
    BackHandler { s.onClose() }
    LaunchedEffect(s.id, s.current?.scannedAtMs) { s.loadFinds() }
    LaunchedEffect(s.finds.bubblePx) { s.finds.refocusOnBubble(density) }
    LaunchedEffect(s.finds.selected) {
        val fid = s.finds.selected ?: return@LaunchedEffect
        val at = view.shown.indexOfFirst { it.id == fid }
        if (at >= 0 && listState.layoutInfo.visibleItemsInfo.none { it.key == fid }) listState.animateScrollToItem(at + HEADER_ITEMS)
    }
}

// The map with the shape being drawn, the finds and the home marker.
@Composable
private fun EditorMap(
    s: RealmEditorState,
    view: FindsView,
    density: Float,
) {
    val m = s.m
    val editing = s.tab == EditorTab.AREA
    QuestMap(
        emptyList(),
        emptyList(),
        if (s.polygon) m.draft.toList() else emptyList(),
        m.me,
        null,
        null,
        null,
        s::onMapTap,
        Modifier.fillMaxSize(),
        home = m.home?.let { LatLng(it.lat, it.lon) },
        onMapLongClick = { m.realmOps.setHome(it) },
        circle = if (s.polygon) null else s.circleCenter?.let { it to s.radius.toDouble() },
        overlayTopDp = OVERLAY_TOP_DP,
        overlayBottomDp = (s.panelPx / density).toInt(),
        handles = s.handles(),
        onHandleMove = if (editing) s::moveHandle else null,
        onHandleRelease = s::commit,
        editable = editing,
        finds = view.mapFinds,
        onFindClick = if (editing) null else { fid -> view.visible.firstOrNull { it.id == fid }?.let { s.finds.show(it, density) } },
        focus = s.finds.focus,
        fit = s.fit,
        anchor = view.visible.firstOrNull { it.id == s.finds.selected }?.let { LatLng(it.at.lat, it.at.lon) },
        onAnchor = { s.anchor = it },
    )
}

// Left: the three modes. Circle and Polygon share a pill (one or the other); Details is its own.
@Composable
private fun BoxScope.ModeTools(s: RealmEditorState) {
    Column(Modifier.align(Alignment.TopStart).padding(top = 12.dp, start = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        ToolPill {
            ToolButton(ApgoIcons.Circle, "Circle", selected = s.tab == EditorTab.AREA && !s.polygon, onClick = { s.chooseShape(false) })
            ToolButton(ApgoIcons.Polygon, "Polygon", selected = s.tab == EditorTab.AREA && s.polygon, onClick = { s.chooseShape(true) })
            if (s.tab == EditorTab.AREA && s.polygon) {
                ToolButton(ApgoIcons.ClearAll, "Clear corners", enabled = s.m.draft.isNotEmpty(), onClick = s::clearCorners)
            }
        }
        ToolPill { ToolButton(ApgoIcons.Finds, "Details", selected = s.tab == EditorTab.DETAILS, onClick = { s.goTab(EditorTab.DETAILS) }) }
    }
}

// Right: history and the way out.
@Composable
private fun BoxScope.HistoryTools(s: RealmEditorState) {
    Row(Modifier.align(Alignment.TopEnd).padding(top = 12.dp, end = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        ToolPillRow {
            ToolButton(ApgoIcons.Undo, "Undo", enabled = s.canUndo, onClick = { s.undo() })
            ToolButton(ApgoIcons.Redo, "Redo", enabled = s.canRedo, onClick = { s.redo() })
        }
        // Done is a real button, not an X: nothing is lost by pressing it, and the cue beside it says so.
        Button(onClick = s.onClose, contentPadding = PaddingValues(horizontal = 14.dp, vertical = 8.dp)) {
            IconLabel("Done", ApgoIcons.Done, textSize = 14.sp)
        }
    }
}

/** The finds the editor shows: those inside the shape, as map pins, and those that match the search and filter. */
internal class FindsView(
    val visible: List<FindOut>,
    val mapFinds: List<MapFind>,
    val shown: List<FindOut>,
)

// Only finds inside the shape being drawn count. What was scanned for an earlier shape can lie outside the new one.
@Composable
private fun rememberFindsView(s: RealmEditorState): FindsView {
    val draftKey = s.m.draft.toList()
    val circleCenter = s.circleCenter
    val visible =
        remember(s.finds.version, s.polygon, s.radius, circleCenter, draftKey) {
            s.finds.all.filter { f -> insideShape(f.at.lat, f.at.lon, s.polygon, circleCenter, s.radius.toDouble(), draftKey) }
        }
    val mapFinds =
        remember(visible, s.finds.selected) {
            visible.map { MapFind(it.id, LatLng(it.at.lat, it.at.lon), it.kindId, it.family, it.mark, it.id == s.finds.selected) }
        }
    val shown =
        remember(visible, s.query, s.filter) { visible.filter { FindFilter.matches(it, s.filter, s.query) } }
    return FindsView(visible, mapFinds, shown)
}
