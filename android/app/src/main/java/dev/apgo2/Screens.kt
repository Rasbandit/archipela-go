package dev.apgo2

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import dev.apgo2.ui.ApgoChip
import dev.apgo2.ui.PLAY_MODES
import dev.apgo2.ui.IconChoices
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.FeedbackText
import dev.apgo2.ui.MapOverlayCard
import dev.apgo2.ui.Tone
import dev.apgo2.ui.modeLabel
import org.maplibre.android.geometry.LatLng
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import dev.apgo2.ui.PreviewFrame
import dev.apgo2.ui.HOME_PREVIEW_ZOOM
import dev.apgo2.ui.HomePreview
import uniffi.apgo_ffi.RealmStatsOut
import dev.apgo2.ui.RealmStatsBox
import dev.apgo2.ui.ScanFigures
import androidx.compose.ui.platform.LocalContext
import dev.apgo2.ui.mapSnapshot
import dev.apgo2.ui.frameFor
import dev.apgo2.ui.PREVIEW_DP
import androidx.compose.runtime.rememberUpdatedState
import dev.apgo2.ui.ToolPillRow
import dev.apgo2.ui.ToolPill
import dev.apgo2.ui.History
import androidx.compose.ui.draw.clip
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.DropdownMenu
import dev.apgo2.ui.ToolButton
import androidx.compose.foundation.shape.RoundedCornerShape
import uniffi.apgo_ffi.RealmOut
import dev.apgo2.ui.circleRing
import dev.apgo2.ui.RealmPreview
import dev.apgo2.ui.PreviewDot
import androidx.compose.runtime.produceState
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.ui.layout.layout
import androidx.compose.material3.CardDefaults
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.material3.IconButton
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.layout.fillMaxHeight
import dev.apgo2.ui.IconLabel
import dev.apgo2.ui.ApgoIcons
import androidx.compose.material3.Icon
import uniffi.apgo_ffi.CircleOut
import uniffi.apgo_ffi.FindOut
import uniffi.apgo_ffi.GeoPoint
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import dev.apgo2.ui.MarkToggle
import dev.apgo2.ui.ChoiceChips
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.draw.alpha
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.SoloOptionsIn

@Composable
fun AppRoot(m: AppModel) {
    m.away?.let { AwayDialog(it) { m.away = null } }
    // Back from New Game or Play goes to Realms; the realm editor handles its own Back (to the list); on the list it leaves the app as usual.
    BackHandler(enabled = m.tab != 0) { m.tab = 0 }
    Scaffold(
        bottomBar = {
            NavigationBar {
                listOf("Realms", "New Game", "Play", "Activity").forEachIndexed { i, t ->
                    NavigationBarItem(selected = m.tab == i, onClick = { m.tab = i; if (i == 0) { m.editing = null; m.pickingHome = false } }, // tapping Realms again leaves the editor
                         icon = { Icon(listOf(ApgoIcons.Realms, ApgoIcons.NewGame, ApgoIcons.Play, ApgoIcons.Activity)[i], contentDescription = t) }, label = { Text(t) })
                }
            }
        },
    ) { pad ->
        Column(Modifier.fillMaxSize().padding(pad)) {
            m.busy?.let {
                Text(it, Modifier.padding(horizontal = 12.dp, vertical = 4.dp), fontSize = 12.sp)
                m.busyFraction?.let { f -> LinearProgressIndicator(progress = { f }, modifier = Modifier.fillMaxWidth()) } ?: LinearProgressIndicator(Modifier.fillMaxWidth())
            }
            if (m.status.isNotBlank()) Text(m.status, Modifier.padding(horizontal = 12.dp, vertical = 2.dp), fontSize = 12.sp, color = MaterialTheme.colorScheme.primary)
            Box(Modifier.weight(1f)) {
                when (m.tab) {
                    0 -> RealmsScreen(m)
                    1 -> NewGameScreen(m)
                    3 -> ActivityScreen(m)
                    else -> PlayScreen(m)
                }
            }
        }
    }
    m.scanAsk?.let { ask ->
        AlertDialog(
            onDismissRequest = { m.scanAsk = null },
            title = { Text("A big area") },
            text = { Text("Finding places here needs about ${ask.requests} downloads (${ask.tiles} map areas) and may take a few minutes. It only has to be done once for each area.") },
            confirmButton = { TextButton(onClick = { m.scanAsk = null; m.scan(ask.id, confirmed = true) }) { Text("Continue") } },
            dismissButton = { TextButton(onClick = { m.scanAsk = null }) { Text("Not now") } },
        )
    }
    m.yamlText?.let { y ->
        val clip = LocalClipboardManager.current
        AlertDialog(
            onDismissRequest = { m.yamlText = null },
            title = { Text("Archipelago YAML") },
            text = { Text(y, fontSize = 11.sp, modifier = Modifier.verticalScroll(rememberScrollState())) },
            confirmButton = { TextButton(onClick = { clip.setText(AnnotatedString(y)); m.status = "YAML copied" ; m.yamlText = null }) { Text("Copy") } },
            dismissButton = { TextButton(onClick = { m.yamlText = null }) { Text("Close") } },
        )
    }
}

// ------------------------------------------------------------------ realms
/** Two views: the list of realms, and a full-page map editor for a new one. */
@Composable
fun RealmsScreen(m: AppModel) {
    if (m.pickingHome) { HomePicker(m) { m.pickingHome = false }; return }
    m.editing?.let { id -> RealmEditor(m, id.ifEmpty { null }) { m.editing = null } } ?: RealmList(m, onNew = { m.editing = "" }, onEdit = { m.editing = it })
}

@Composable
private fun RealmList(m: AppModel, onNew: () -> Unit, onEdit: (String) -> Unit) {
    val snackbar = remember { SnackbarHostState() }
    var asking by remember { mutableStateOf<RealmOut?>(null) }
    ConfirmDelete(asking, onConfirm = { r -> asking = null; m.deleteWithUndo(r.id) }, onDismiss = { asking = null })
    // Show an Undo bar for each delete; when it goes away without Undo (or the screen is left) the realm is really deleted.
    LaunchedEffect(m.undo) {
        val u = m.undo ?: return@LaunchedEffect
        var undone = false
        try {
            undone = snackbar.showSnackbar("Deleted ${u.name}", actionLabel = "Undo", duration = SnackbarDuration.Short) == SnackbarResult.ActionPerformed
        } finally {
            if (undone) m.undoDelete(u.id) else m.commitDelete(u.id)
        }
    }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize().padding(start = 8.dp, end = 8.dp, top = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            HomeCard(m) { m.pickingHome = true }
            Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                Text("Realms", style = MaterialTheme.typography.titleMedium)
                Button(onClick = onNew) { IconLabel("New realm", ApgoIcons.Add) }
            }
            if (m.shownRealms.isEmpty()) Text("No realms yet. Tap New realm to draw one.", fontSize = 13.sp)
            LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(m.shownRealms, key = { it.id }) { r ->
                    val dismiss = rememberSwipeToDismissBoxState(confirmValueChange = { if (it == SwipeToDismissBoxValue.EndToStart) asking = r; false })
                    SwipeToDismissBox(
                        state = dismiss,
                        enableDismissFromStartToEnd = false,
                        backgroundContent = {
                            Box(Modifier.fillMaxSize().clip(RoundedCornerShape(12.dp)).background(ApgoPalette.danger).padding(end = 20.dp), contentAlignment = Alignment.CenterEnd) {
                                Icon(ApgoIcons.Delete, contentDescription = "Delete", tint = Color.White)
                            }
                        },
                    ) { RealmCard(m, r) { onEdit(r.id) } }
                }
            }
        }
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter))
    }
}

/**
 * Where distances are measured from. It looks different from a realm on purpose (a green outline and a house), so it is never mistaken for one:
 * a small map of the spot, and a way to move it.
 */
@Composable
private fun HomeCard(m: AppModel, onClick: () -> Unit) {
    val context = LocalContext.current
    val home = m.home?.let { LatLng(it.lat, it.lon) }
    val map by produceState<android.graphics.Bitmap?>(null, home) {
        value = null
        value = home?.let { runCatching { mapSnapshot(context, PreviewFrame(it, HOME_PREVIEW_ZOOM)) }.getOrNull() }
    }
    Card(
        Modifier.fillMaxWidth().clickable(onClick = onClick),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = androidx.compose.foundation.BorderStroke(2.dp, ApgoPalette.home),
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(ApgoIcons.Home, contentDescription = null, tint = ApgoPalette.home, modifier = Modifier.size(26.dp))
                    Text("Home", style = MaterialTheme.typography.titleMedium)
                }
                Text(
                    if (home == null) "Not set yet. Tap to choose where distances are measured from." else "Distances are measured from here. Tap to move it.",
                    fontSize = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (home != null) HomePreview(map, Modifier.size(PREVIEW_DP.dp))
        }
    }
}

/** A realm at a glance: its icon and name, how many finds and quest types it offers, and a small preview of the region. */
@Composable
private fun RealmCard(m: AppModel, r: RealmOut, onClick: () -> Unit) {
    val types = m.offers[r.id].orEmpty().size
    val dots by produceState(emptyList<PreviewDot>(), r.id, r.scannedAtMs, types) {
        value = if (r.scannedAtMs == null) emptyList() else withContext(Dispatchers.IO) {
            m.engine.realmDots(r.id, 90u).map { PreviewDot(it.at.lat, it.at.lon, ApgoPalette.kind(it.kindId, it.family)) }
        }
    }
    val outline = if (r.polygonActive) r.polygon.map { it.lat to it.lon } else r.circle?.let { circleRing(it.center.lat, it.center.lon, it.radiusM) }.orEmpty()
    val context = LocalContext.current
    val frame = frameFor(outline)
    // The picture belongs to one frame: when the shape changes it is dropped at once (never shown under a different outline) and drawn again.
    val map by produceState<android.graphics.Bitmap?>(null, frame?.key) {
        value = null
        value = frame?.let { runCatching { mapSnapshot(context, it) }.getOrNull() }
    }
    Card(Modifier.fillMaxWidth().clickable(onClick = onClick)) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(ApgoIcons.realm(r.icon), contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(26.dp))
                    Text(r.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                if (r.scannedAtMs == null) {
                    Text("Not scanned yet", fontSize = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                } else {
                    Text("${r.places} finds", style = MaterialTheme.typography.bodyLarge)
                    Text("$types quest types", fontSize = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            RealmPreview(outline, dots, map, Modifier.size(PREVIEW_DP.dp))
        }
    }
}

/**
 * Choose home on a map. The pin can be dragged, the map tapped to put it there, or "My location" pressed. Each placement is saved at once, so
 * Done only closes.
 */
@Composable
private fun HomePicker(m: AppModel, onClose: () -> Unit) {
    val start = remember { m.home?.let { LatLng(it.lat, it.lon) } ?: m.me ?: m.shownRealms.firstOrNull()?.let { r -> r.circle?.let { LatLng(it.center.lat, it.center.lon) } ?: r.polygon.firstOrNull()?.let { LatLng(it.lat, it.lon) } } }
    var pin by remember { mutableStateOf(start) }
    var saved by remember { mutableStateOf(m.home != null) }
    var focus by remember { mutableStateOf<MapFocus?>(null) }
    var nonce by remember { mutableIntStateOf(0) }
    // Open showing your realms and the pin together, so the pin can be judged against the places you play.
    val framing = remember {
        val pts = m.shownRealms.flatMap { r ->
            if (r.polygonActive) r.polygon.map { LatLng(it.lat, it.lon) }
            else r.circle?.let { c ->
                val dLat = c.radiusM / 111_195.0
                val dLon = dLat / kotlin.math.cos(Math.toRadians(c.center.lat))
                listOf(LatLng(c.center.lat + dLat, c.center.lon), LatLng(c.center.lat - dLat, c.center.lon), LatLng(c.center.lat, c.center.lon + dLon), LatLng(c.center.lat, c.center.lon - dLon))
            }.orEmpty()
        } + listOfNotNull(start)
        if (pts.size >= 2) MapFit(pts, 1) else null
    }
    fun place(to: LatLng) { pin = to; m.setHome(to, announce = false); saved = true }
    BackHandler { onClose() }

    Box(Modifier.fillMaxSize()) {
        QuestMap(
            emptyList(), m.shownRealms, emptyList(), m.me, null, null, null, { place(it) },
            Modifier.fillMaxSize(),
            home = pin,
            handles = listOfNotNull(pin), handlesVisible = false, onHandleMove = { _, to -> pin = to }, onHandleRelease = { pin?.let { place(it) } },
            focus = focus, fit = framing,
            overlayTopDp = 16, overlayBottomDp = 150,
        )
        Row(Modifier.align(Alignment.TopEnd).padding(top = 12.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = onClose, contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 14.dp, vertical = 8.dp)) { IconLabel("Done", ApgoIcons.Done, 14.sp) }
        }
        if (saved) {
            Row(
                Modifier.align(Alignment.TopEnd).padding(top = 72.dp, end = 16.dp).background(MaterialTheme.colorScheme.surface.copy(alpha = 0.9f), RoundedCornerShape(12.dp)).padding(horizontal = 8.dp, vertical = 2.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Icon(ApgoIcons.Saved, contentDescription = null, tint = ApgoPalette.success, modifier = Modifier.size(14.dp))
                Text("Home saved", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        MapOverlayCard(Modifier.align(Alignment.BottomCenter)) {
            Text("Home", style = MaterialTheme.typography.titleSmall)
            Text(
                if (pin == null) "Tap the map to put your home there." else "Drag the pin or tap the map to move it. Distances in your games are measured from here.",
                fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button(onClick = { m.me?.let { place(it); focus = MapFocus(it, ++nonce) } }, enabled = m.me != null, modifier = Modifier.fillMaxWidth()) {
                IconLabel(if (m.me == null) "Waiting for your location…" else "Use my location", ApgoIcons.Me, 14.sp)
            }
        }
    }
}

/** Everything the editor can change, so a step of Undo/Redo can restore it whole. */
private data class EditSnap(val polygon: Boolean, val radius: Float, val center: LatLng?, val corners: List<LatLng>, val name: String, val icon: String?)

/**
 * The realm editor. The map is the whole screen. A toolbar on the left switches between editing the area (Circle or Polygon, one or the other)
 * and Details (name, icon and the finds the scan found); Undo, Redo and a close button sit on the right. Every finished edit is saved at once,
 * so there is no Save or Cancel: Undo goes back. A new realm is created by its first edit and named "Realm N". [realmId] null starts a new realm.
 */
@Composable
private fun RealmEditor(m: AppModel, realmId: String?, onClose: () -> Unit) {
    val original = remember(realmId) { m.realms.firstOrNull { it.id == realmId } }
    var id by remember(realmId) { mutableStateOf(realmId) } // set when a new realm is first saved
    val current = m.realms.firstOrNull { it.id == id }
    var tab by remember(realmId) { mutableStateOf(AREA) }
    var name by remember(realmId) { mutableStateOf(original?.name ?: "") }
    var icon by remember(realmId) { mutableStateOf(original?.icon) }
    var pickingIcon by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf<RealmOut?>(null) }
    var deleted by remember { mutableStateOf(false) }
    var polygon by remember(realmId) { mutableStateOf(original?.polygonActive == true) }
    var radius by remember(realmId) { mutableFloatStateOf(original?.circle?.radiusM?.toFloat() ?: 1500f) }
    var center by remember(realmId) { mutableStateOf(original?.circle?.let { LatLng(it.center.lat, it.center.lon) }) }
    remember(realmId) { m.draft.clear(); original?.polygon?.forEach { m.draft.add(LatLng(it.lat, it.lon)) } }
    val circleCenter = center ?: m.me // a new circle follows your GPS until it is edited

    // ---- history and autosave
    fun snap() = EditSnap(polygon, radius, center, m.draft.toList(), name, icon)
    val history = remember(realmId) { History(snap()) }
    var savedAt by remember(realmId) { mutableStateOf<Long?>(null) } // when the realm was last written to disk
    // The outline the finds were last fetched for. The shape counts as changed only while it differs from that one, so undoing back to it is not a change.
    fun outlineKey() = EditSnap(polygon, radius, center, m.draft.toList(), "", null)
    var scannedKey by remember(realmId) { mutableStateOf(if (original?.scannedAtMs != null) EditSnap(original.polygonActive, original.circle?.radiusM?.toFloat() ?: 1500f, original.circle?.let { LatLng(it.center.lat, it.center.lon) }, original.polygon.map { LatLng(it.lat, it.lon) }, "", null) else null) }
    val shapeDirty = outlineKey() != scannedKey

    /** Save what is on screen. Returns false when there is nothing to save yet (no location, or a polygon is not drawn). */
    fun persist(): Boolean {
        if (deleted) return false
        val c = center ?: m.me // read now: the value captured when the screen was last drawn may be older than this edit
        val rid = m.saveRealm(id, name, icon, c?.let { it to radius.toDouble() }, m.draft.toList(), polygonActive = polygon && m.draft.size >= 3) ?: return false
        if (id == null) {
            id = rid
            name = m.realms.firstOrNull { it.id == rid }?.name ?: name // the default "Realm N"
        }
        savedAt = System.currentTimeMillis()
        return true
    }

    /** A finished edit: freeze a following circle in place, save, and record it for Undo. */
    fun commit(shapeEdit: Boolean) {
        if (center == null && !polygon) center = m.me
        if (persist()) history.push(snap())
    }

    fun apply(s: EditSnap) {
        polygon = s.polygon; radius = s.radius; center = s.center; name = s.name.ifBlank { name }; icon = s.icon // the first state has no name yet: keep the default
        m.draft.clear(); m.draft.addAll(s.corners)
        persist()
    }

    // Typing a name is saved once you pause.
    LaunchedEffect(name) {
        if (name == history.current.name && id != null) return@LaunchedEffect
        if (name.isBlank() && id == null) return@LaunchedEffect
        kotlinx.coroutines.delay(600)
        commit(false)
    }

    // However the editor is left (X, Back, another tab): keep a pending name, and fetch finds for an outline that changed.
    val leave by rememberUpdatedState {
        if (!deleted) {
            if (name != history.current.name) commit(false)
            val rid = id
            if (rid != null && outlineKey() != scannedKey) m.scan(rid)
        }
        m.draft.clear()
    }
    DisposableEffect(Unit) { onDispose { leave() } }
    BackHandler { onClose() }

    // ---- finds: what the scan found. Pins on the map, rows in Details.
    val finds = remember(realmId) { mutableStateListOf<FindOut>() }
    var findsVersion by remember(realmId) { mutableIntStateOf(0) }
    var selectedFind by remember(realmId) { mutableStateOf<String?>(null) }
    var focus by remember(realmId) { mutableStateOf<MapFocus?>(null) }
    var focusNonce by remember { mutableIntStateOf(0) }
    var anchor by remember(realmId) { mutableStateOf<androidx.compose.ui.geometry.Offset?>(null) }
    var query by remember(realmId) { mutableStateOf("") }
    var filter by remember(realmId) { mutableStateOf(ALL) }
    LaunchedEffect(id, current?.scannedAtMs) {
        val rid = id
        if (rid != null && current?.scannedAtMs != null) {
            val all = withContext(Dispatchers.IO) { m.engine.realmFinds(rid) }
            finds.clear(); finds.addAll(all); findsVersion++
        }
    }
    // Only finds inside the shape being drawn count. What was scanned for an earlier shape can lie outside the new one.
    val draftKey = m.draft.toList()
    val visible = remember(findsVersion, polygon, radius, circleCenter, draftKey) {
        finds.filter { f -> insideShape(f.at.lat, f.at.lon, polygon, circleCenter, radius.toDouble(), draftKey) }
    }
    val mapFinds = remember(visible, selectedFind) { visible.map { MapFind(it.id, LatLng(it.at.lat, it.at.lon), it.kindId, it.family, it.mark, it.id == selectedFind) } }
    val shown = remember(visible, query, filter) {
        visible.filter { f -> (filter == ALL || f.mark == filter) && (query.isBlank() || f.name.contains(query, true) || f.kinds.any { it.name.contains(query, true) }) }
    }
    fun mark(f: FindOut, to: String) {
        val next = if (f.mark == to) "none" else to // tapping a lit toggle clears it
        val rid = id ?: return
        if (m.setFindMark(rid, f.id, next)) { finds[finds.indexOfFirst { it.id == f.id }] = f.copy(mark = next); findsVersion++ }
    }
    // Selecting a find brings it into view together with its callout. The callout's real height is measured once it is shown.
    val screenDensity = LocalDensity.current.density
    var bubblePx by remember { mutableIntStateOf(0) }
    fun roomAbove() = (if (bubblePx > 0) bubblePx else (230 * screenDensity).toInt()) + (26 * screenDensity).toInt()
    fun show(f: FindOut) {
        selectedFind = f.id
        focus = MapFocus(LatLng(f.at.lat, f.at.lon), ++focusNonce, roomAbove())
    }
    LaunchedEffect(bubblePx) {
        val f = finds.firstOrNull { it.id == selectedFind } ?: return@LaunchedEffect
        if (bubblePx > 0) focus = MapFocus(LatLng(f.at.lat, f.at.lon), ++focusNonce, roomAbove())
    }

    // A circle has a handle at its center (moves it); its whole ring is an invisible handle (resizes it). A polygon has one per corner.
    val handles = if (tab == AREA) { if (polygon) m.draft.toList() else listOfNotNull(circleCenter) } else emptyList()
    fun moveHandle(i: Int, to: LatLng) {
        if (polygon) { if (i in m.draft.indices) m.draft[i] = to; return }
        val c = circleCenter ?: return
        if (i == 0) center = to
        else {
            val d = floatArrayOf(0f)
            android.location.Location.distanceBetween(c.latitude, c.longitude, to.latitude, to.longitude, d)
            radius = d[0].coerceIn(300f, 8000f)
        }
    }

    // The map keeps clear of whatever panel is showing by padding itself with its real height.
    var panelPx by remember { mutableIntStateOf(0) }
    val panelDp = (panelPx / LocalDensity.current.density).toInt()
    var fit by remember(realmId) { mutableStateOf<MapFit?>(null) }
    var fitNonce by remember { mutableIntStateOf(0) }
    fun shapePoints(): List<LatLng> {
        if (polygon) return m.draft.toList()
        val c = circleCenter ?: return emptyList()
        val dLat = radius / 111_195.0
        val dLon = radius / (111_195.0 * kotlin.math.cos(Math.toRadians(c.latitude)))
        return listOf(LatLng(c.latitude + dLat, c.longitude), LatLng(c.latitude - dLat, c.longitude), LatLng(c.latitude, c.longitude + dLon), LatLng(c.latitude, c.longitude - dLon))
    }
    fun goTab(to: Int) {
        if (to == tab) return
        if (to == DETAILS) {
            // Opening Details creates a new realm if need be, and fetches finds for an outline that is new or changed.
            if (id == null) commit(true)
            val rid = id
            if (rid != null && (outlineKey() != scannedKey || current?.scannedAtMs == null)) { m.scan(rid); scannedKey = outlineKey() }
        } else {
            // Back to the area: after the map settles, fit the whole shape in the view.
            fit = MapFit(shapePoints(), ++fitNonce); selectedFind = null
        }
        tab = to
    }
    val listState = rememberLazyListState()
    LaunchedEffect(selectedFind) {
        val fid = selectedFind ?: return@LaunchedEffect
        val at = shown.indexOfFirst { it.id == fid }
        if (at >= 0 && listState.layoutInfo.visibleItemsInfo.none { it.key == fid }) listState.animateScrollToItem(at + HEADER_ITEMS)
    }

    Box(Modifier.fillMaxSize()) {
        QuestMap(
            emptyList(), emptyList(), if (polygon) m.draft.toList() else emptyList(), m.me, null, null, null,
            { if (polygon && tab == AREA) { m.draft.add(it); commit(true) } },
            Modifier.fillMaxSize(),
            home = m.home?.let { LatLng(it.lat, it.lon) },
            onMapLongClick = { m.setHome(it) },
            circle = if (polygon) null else circleCenter?.let { it to radius.toDouble() },
            overlayTopDp = 16, overlayBottomDp = panelDp,
            handles = handles, onHandleMove = if (tab == AREA) ::moveHandle else null, onHandleRelease = { commit(true) }, editable = tab == AREA,
            finds = mapFinds,
            onFindClick = if (tab == DETAILS) { fid -> visible.firstOrNull { it.id == fid }?.let(::show) } else null,
            focus = focus,
            fit = fit,
            anchor = visible.firstOrNull { it.id == selectedFind }?.let { LatLng(it.at.lat, it.at.lon) },
            onAnchor = { anchor = it },
        )
        RealmEditorDialogs(pickingIcon, icon, { icon = it; commit(false) }, { pickingIcon = false })
        ConfirmDelete(confirmDelete, onConfirm = { r -> confirmDelete = null; deleted = true; m.deleteWithUndo(r.id); onClose() }, onDismiss = { confirmDelete = null })
        visible.firstOrNull { it.id == selectedFind }?.let { f ->
            anchor?.let { at -> FindBubble(f, at, onSize = { bubblePx = it.height }, onMark = { mark(f, it) }, onClose = { selectedFind = null }) }
        }

        // Left: the three modes. Circle and Polygon share a pill (one or the other); Details is its own.
        Column(Modifier.align(Alignment.TopStart).padding(top = 12.dp, start = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            ToolPill {
                ToolButton(ApgoIcons.Circle, "Circle", selected = tab == AREA && !polygon) {
                    val changed = polygon
                    polygon = false; goTab(AREA); if (changed) commit(true)
                }
                ToolButton(ApgoIcons.Polygon, "Polygon", selected = tab == AREA && polygon) {
                    val changed = !polygon
                    polygon = true; goTab(AREA); if (changed) commit(true)
                }
                if (tab == AREA && polygon) ToolButton(ApgoIcons.ClearAll, "Clear corners", enabled = m.draft.isNotEmpty()) { m.draft.clear(); commit(true) }
            }
            ToolPill { ToolButton(ApgoIcons.Finds, "Details", selected = tab == DETAILS) { goTab(DETAILS) } }
        }
        // Right: history and the way out.
        Row(Modifier.align(Alignment.TopEnd).padding(top = 12.dp, end = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ToolPillRow {
                ToolButton(ApgoIcons.Undo, "Undo", enabled = history.canUndo) { history.undo()?.let(::apply) }
                ToolButton(ApgoIcons.Redo, "Redo", enabled = history.canRedo) { history.redo()?.let(::apply) }
            }
            // Done is a real button, not an X: nothing is lost by pressing it, and the cue beside it says so.
            Button(onClick = onClose, contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 14.dp, vertical = 8.dp)) { IconLabel("Done", ApgoIcons.Done, 14.sp) }
        }

        savedAt?.let {
            Row(
                Modifier.align(Alignment.TopEnd).padding(top = 72.dp, end = 16.dp).background(MaterialTheme.colorScheme.surface.copy(alpha = 0.9f), RoundedCornerShape(12.dp)).padding(horizontal = 8.dp, vertical = 2.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Icon(ApgoIcons.Saved, contentDescription = null, tint = ApgoPalette.success, modifier = Modifier.size(14.dp))
                Text("All changes saved", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        if (tab == AREA) {
            // Area is just the map and a box of numbers about what is chosen.
            MapOverlayCard(Modifier.align(Alignment.BottomCenter).onSizeChanged { panelPx = it.height }) {
                if (polygon && m.draft.size < 3) {
                    Text("Tap the map to add corners (${m.draft.size} of at least 3).", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                } else {
                    val circleOut = circleCenter?.let { CircleOut(GeoPoint(it.latitude, it.longitude), radius.toDouble()) }
                    val corners = m.draft.toList()
                    val shape = remember(polygon, radius, circleCenter, corners, m.home) {
                        m.engine.shapeStats(circleOut, corners.map { GeoPoint(it.latitude, it.longitude) }, polygon && corners.size >= 3, m.home)
                    }
                    // Figures from the last scan. After the outline changes they are the old ones, marked as such until Details refreshes them.
                    var found by remember(id) { mutableStateOf<RealmStatsOut?>(null) }
                    LaunchedEffect(id, current?.scannedAtMs) {
                        val rid = id
                        found = if (rid != null && current?.scannedAtMs != null) withContext(Dispatchers.IO) { m.engine.realmStats(rid) } else null
                    }
                    val waiting = if (m.busy != null) "looking…" else "after scan"
                    RealmStatsBox(shape.areaM2, shape.farthestM, found?.let { ScanFigures(it.walkableM, it.streets.toInt(), it.trailM, it.finds.toInt(), it.parks.toInt(), it.roughShare, stale = shapeDirty) }, waiting)
                }
            }
        } else {
            MapOverlayCard(Modifier.align(Alignment.BottomCenter).onSizeChanged { panelPx = it.height }.fillMaxHeight(0.5f), fillHeight = true) {
                LazyColumn(Modifier.weight(1f).fillMaxWidth(), state = listState) {
                    // A compact header so the finds get most of the half-height panel: icon + name, search + filters, then the count.
                    item(key = "name") {
                        Row(Modifier.fillMaxWidth().padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            IconButton(onClick = { pickingIcon = true }) { Icon(ApgoIcons.realm(icon), contentDescription = "Choose an icon", tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(28.dp)) }
                            OutlinedTextField(name, { name = it }, label = { Text("Realm name") }, singleLine = true, modifier = Modifier.weight(1f))
                        }
                    }
                    item(key = "search") {
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            OutlinedTextField(query, { query = it }, label = { Text("Search finds") }, singleLine = true, modifier = Modifier.weight(1f))
                            IconChoices(
                                listOf(ALL, FAVORITE, BANNED), filter, { filter = it },
                                { when (it) { FAVORITE -> ApgoIcons.Favorite; BANNED -> ApgoIcons.Banned; else -> ApgoIcons.All } },
                                { when (it) { ALL -> "Show all finds"; FAVORITE -> "Show favorites"; else -> "Show banned" } },
                            )
                        }
                    }
                    item(key = "count") {
                        Text(
                            when {
                                m.busy != null -> "Looking for finds…"
                                current?.scannedAtMs == null -> "No finds yet."
                                findsVersion == 0 -> "Loading finds…"
                                else -> "${shown.size} of ${visible.size} finds · ${visible.count { it.mark == FAVORITE }} favorites · ${visible.count { it.mark == BANNED }} banned"
                            },
                            Modifier.padding(vertical = 4.dp), fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        current?.let { r ->
                            TextButton(onClick = { confirmDelete = r }) {
                                Icon(ApgoIcons.Delete, contentDescription = null, tint = ApgoPalette.danger, modifier = Modifier.size(16.dp))
                                Text("  Delete realm", fontSize = 12.sp, color = ApgoPalette.danger)
                            }
                        }
                    }
                    items(shown, key = { it.id }) { f ->
                        Row(
                            Modifier.fillMaxWidth().background(if (f.id == selectedFind) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent).clickable { show(f) },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(
                                ApgoIcons.forKind(f.kindId, f.family), contentDescription = null, modifier = Modifier.padding(horizontal = 8.dp).size(22.dp),
                                tint = if (f.mark == BANNED) ApgoPalette.muted else ApgoPalette.kind(f.kindId, f.family),
                            )
                            Column(Modifier.weight(1f).alpha(if (f.mark == BANNED) 0.5f else 1f)) {
                                Text(
                                    f.name, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium,
                                    textDecoration = if (f.mark == BANNED) TextDecoration.LineThrough else null,
                                )
                                // An unnamed find is titled by its first quest kind, so the subtitle must not repeat it.
                                Text(
                                    (if (f.named) f.kinds.map { it.name } else listOf("unnamed") + f.kinds.drop(1).map { it.name }).joinToString(", ") + " · ${distanceLabel(f.distanceM)}",
                                    maxLines = 1, overflow = TextOverflow.Ellipsis, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            MarkToggle(ApgoIcons.Favorite, "Favorite", f.mark == FAVORITE, ApgoPalette.favorite) { mark(f, FAVORITE) }
                            MarkToggle(ApgoIcons.Banned, "Ban", f.mark == BANNED, ApgoPalette.banned) { mark(f, BANNED) }
                        }
                        HorizontalDivider()
                    }
                }
            }
        }
    }
}

/** Asks before a realm is deleted (swipe or editor). [realm] null shows nothing. */
@Composable
private fun ConfirmDelete(realm: RealmOut?, onConfirm: (RealmOut) -> Unit, onDismiss: () -> Unit) {
    val r = realm ?: return
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Delete ${r.name}?") },
        text = { Text("Its finds and your favorites and bans go with it. Games already started keep their quests.") },
        confirmButton = { TextButton(onClick = { onConfirm(r) }) { Text("Delete", color = ApgoPalette.danger) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** The dialogs of the realm editor: pick an icon. */
@Composable
private fun RealmEditorDialogs(
    picking: Boolean, current: String?, onPick: (String) -> Unit, onDismissPicker: () -> Unit,
) {
    if (picking) {
        AlertDialog(
            onDismissRequest = onDismissPicker,
            title = { Text("Choose an icon") },
            text = {
                @OptIn(ExperimentalLayoutApi::class)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    ApgoIcons.realmChoices.forEach { (key, vector) ->
                        val chosen = key == (current ?: "pin")
                        IconButton(
                            onClick = { onPick(key) },
                            colors = IconButtonDefaults.iconButtonColors(containerColor = if (chosen) MaterialTheme.colorScheme.primary else Color.Transparent, contentColor = if (chosen) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface),
                        ) { Icon(vector, contentDescription = key) }
                    }
                }
            },
            confirmButton = { TextButton(onClick = onDismissPicker) { Text("Done") } },
        )
    }
}

/** A callout over the map for the selected find: what it is, what the quests mean, and how to complete them. */
@Composable
private fun FindBubble(f: FindOut, at: androidx.compose.ui.geometry.Offset, onSize: (androidx.compose.ui.unit.IntSize) -> Unit, onMark: (String) -> Unit, onClose: () -> Unit) {
    val margin = with(LocalDensity.current) { 8.dp.roundToPx() }
    val gap = with(LocalDensity.current) { 26.dp.roundToPx() }
    val maxWidth = with(LocalDensity.current) { 300.dp.roundToPx() }
    Box(
        Modifier.layout { measurable, constraints ->
            val p = measurable.measure(constraints.copy(minWidth = 0, minHeight = 0, maxWidth = minOf(maxWidth, constraints.maxWidth - 2 * margin)))
            layout(constraints.maxWidth, constraints.maxHeight) {
                // Above the pin when it fits, else below it, and always inside the screen.
                val x = (at.x - p.width / 2f).toInt().coerceIn(margin, maxOf(margin, constraints.maxWidth - p.width - margin))
                val above = at.y - p.height - gap
                p.place(x, if (above >= margin) above.toInt() else (at.y + gap / 2).toInt())
            }
        },
    ) {
        Card(Modifier.onSizeChanged(onSize), elevation = CardDefaults.cardElevation(6.dp)) {
            Column(Modifier.padding(start = 12.dp, top = 8.dp, bottom = 10.dp, end = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(ApgoIcons.forKind(f.kindId, f.family), contentDescription = null, tint = ApgoPalette.kind(f.kindId, f.family), modifier = Modifier.size(24.dp))
                    Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
                        Text(f.name, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Text("${distanceLabel(f.distanceM)} from home" + if (f.named) "" else " · unnamed", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    MarkToggle(ApgoIcons.Favorite, "Favorite", f.mark == FAVORITE, ApgoPalette.favorite) { onMark(FAVORITE) }
                    MarkToggle(ApgoIcons.Banned, "Ban", f.mark == BANNED, ApgoPalette.banned) { onMark(BANNED) }
                    IconButton(onClick = onClose) { Icon(ApgoIcons.Close, contentDescription = "Close") }
                }
                f.kinds.take(2).forEach { k ->
                    Column(Modifier.padding(end = 8.dp)) {
                        Text(k.name, style = MaterialTheme.typography.labelLarge, color = ApgoPalette.kind(k.id, k.family))
                        Text("${k.blurb} ${k.how}", fontSize = 12.sp, lineHeight = 16.sp)
                    }
                }
                if (f.kinds.size > 2) Text("+ ${f.kinds.size - 2} more quest types", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (f.tags.isNotEmpty()) Text("Mapped as ${f.tags.joinToString(" · ")}", fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(end = 8.dp))
            }
        }
    }
}

/** Whether a point lies inside the shape being edited: the circle, or the polygon when it has 3 or more corners. */
private fun insideShape(lat: Double, lon: Double, polygon: Boolean, center: LatLng?, radiusM: Double, corners: List<LatLng>): Boolean {
    if (polygon) {
        if (corners.size < 3) return true // nothing drawn yet: do not hide everything
        var inside = false
        var j = corners.lastIndex
        for (i in corners.indices) { // ray casting
            val (a, b) = corners[i] to corners[j]
            if ((a.latitude > lat) != (b.latitude > lat) && lon < (b.longitude - a.longitude) * (lat - a.latitude) / (b.latitude - a.latitude) + a.longitude) inside = !inside
            j = i
        }
        return inside
    }
    val c = center ?: return true
    val dy = (lat - c.latitude) * 111_195.0
    val dx = (lon - c.longitude) * 111_195.0 * kotlin.math.cos(Math.toRadians(c.latitude))
    return dx * dx + dy * dy <= radiusM * radiusM
}

private const val AREA = 0
private const val DETAILS = 1
private const val HEADER_ITEMS = 3 // name + mode, search + filters, count

private const val ALL = "all"
private const val FAVORITE = "favorite"
private const val BANNED = "banned"

private fun distanceLabel(m: Double) = if (m < 1000) "${m.toInt()} m" else "%.1f km".format(m / 1000)

// -------------------------------------------------------------------- play
@Composable
fun PlayScreen(m: AppModel) {
    val hud = m.hud
    if (hud == null) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("Play", style = MaterialTheme.typography.titleLarge)
            Text("No game is open, so nothing is being tracked.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            GamesList(m)
            OutlinedButton(onClick = { m.tab = 1 }) { Text("New game") }
        }
        return
    }
    val selected = m.quests.firstOrNull { it.locationId == m.selected }
    Column(Modifier.fillMaxSize().padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Text("${hud.gameName}  ·  ${hud.backend}", fontSize = 12.sp)
            OutlinedButton(onClick = { m.pause() }) {
                Icon(ApgoIcons.Pause, contentDescription = null, modifier = Modifier.size(16.dp))
                Text(" Pause tracking", fontSize = 12.sp)
            }
        }
        if (hud.goals.size > 1) {
            // Several goals: the rule and overall progress, then each goal with its own bar.
            Text(hud.goalLabel.substringBefore(":"), style = MaterialTheme.typography.titleSmall)
            LinearProgressIndicator(progress = { hud.goalProgress }, Modifier.fillMaxWidth())
            hud.goals.forEach { g ->
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Icon(if (g.achieved) ApgoIcons.Check else ApgoIcons.Play, contentDescription = if (g.achieved) "Done" else "Not done", tint = if (g.achieved) ApgoPalette.success else ApgoPalette.muted, modifier = Modifier.size(14.dp))
                    Column(Modifier.weight(1f)) {
                        Text(g.label, fontSize = 11.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        LinearProgressIndicator(progress = { g.progress }, Modifier.fillMaxWidth())
                    }
                }
            }
        } else {
            Text(hud.goalLabel, style = MaterialTheme.typography.titleSmall)
            LinearProgressIndicator(progress = { hud.goalProgress }, Modifier.fillMaxWidth())
        }
        Text(
            "Quests ${hud.done}/${hud.total} · keys ${hud.keys} · ${hud.tools.joinToString().ifBlank { "no tools" }} · letters ${hud.letters.ifBlank { "-" }} · ${"%.1f".format(hud.distanceKm)} km · streak ${hud.streakDays}d",
            fontSize = 11.sp,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            m.zones.forEach { z ->
                val tint = if (z.unlocked) ApgoPalette.success else ApgoPalette.muted
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
                    Icon(ApgoIcons.mode(z.mode), contentDescription = null, tint = tint, modifier = Modifier.size(13.dp))
                    Text("Z${z.id}", fontSize = 11.sp, color = tint)
                    Icon(if (z.unlocked) ApgoIcons.Unlocked else ApgoIcons.Locked, contentDescription = if (z.unlocked) "Unlocked" else "Locked", tint = tint, modifier = Modifier.size(13.dp))
                    if (!z.unlocked) Text("${if (z.keysNeeded > 0u) "${z.keysNeeded}key" else ""}${z.tool?.let { "+$it" } ?: ""}", fontSize = 11.sp, color = tint)
                }
            }
        }
        (hud.traps + listOfNotNull(hud.blocked)).distinct().takeIf { it.isNotEmpty() }?.let { FeedbackText(it.joinToString("  ·  "), Tone.Danger) }
        QuestMap(
            m.quests, m.realms.filter { r -> m.zones.any { it.realmId == r.id } }, emptyList(), m.me,
            hud.thaw?.let { org.maplibre.android.geometry.LatLng(it.lat, it.lon) }, hud.waypoint?.let { org.maplibre.android.geometry.LatLng(it.lat, it.lon) },
            m.selected, { ll ->
                m.quests.filter { it.anchor != null && it.state != "hidden" }.minByOrNull { q ->
                    val a = q.anchor!!; val d = floatArrayOf(0f)
                    android.location.Location.distanceBetween(ll.latitude, ll.longitude, a.lat, a.lon, d); d[0]
                }?.let { m.selected = it.locationId }
            },
            Modifier.fillMaxWidth().height(260.dp),
            home = m.home?.let { LatLng(it.lat, it.lon) },
            trace = m.trace,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            OutlinedButton(onClick = { m.devTeleportNext() }) { Text("DEV: do next", fontSize = 11.sp) }
            OutlinedButton(onClick = { m.simPos = null }) { Text("Real GPS", fontSize = 11.sp) }
            OutlinedButton(onClick = { runCatching { m.engine.reroll(emptyList(), (kotlin.random.Random.nextLong() ushr 1).toULong()) }; m.refreshPlay() }) { Text("Reroll all", fontSize = 11.sp) }
        }
        selected?.let { q ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(8.dp)) {
                    Text("${q.name}${if (q.boss) "  (BOSS)" else ""}", style = MaterialTheme.typography.titleSmall)
                    Text("${q.place} · ${q.difficulty} · ~${q.effortMin.toInt()} min · ${q.mode}${if (q.fallback) " · fallback" else ""}", fontSize = 11.sp)
                    Text(q.detail, fontSize = 12.sp)
                    Text(q.blurb, fontSize = 11.sp)
                    q.reward?.let { FeedbackText("Reward: $it", Tone.Success) }
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        OutlinedButton(onClick = { m.devComplete(q) }) { Text("DEV: complete", fontSize = 11.sp) }
                        if (q.state != "done") OutlinedButton(onClick = { runCatching { m.engine.reroll(listOf(q.locationId), (kotlin.random.Random.nextLong() ushr 1).toULong()) }; m.refreshPlay() }) { Text("Reroll", fontSize = 11.sp) }
                    }
                }
            }
        }
        val order = listOf("progress", "open", "locked", "done", "hidden")
        LazyColumn(Modifier.weight(1f)) {
            items(m.quests.sortedBy { order.indexOf(it.state) }, key = { it.locationId }) { q ->
                Row(Modifier.fillMaxWidth().clickable { m.selected = q.locationId }.padding(vertical = 3.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(ApgoIcons.forKind(q.kindId, q.family), contentDescription = null, tint = ApgoPalette.quest(q.state), modifier = Modifier.size(20.dp))
                    Column(Modifier.weight(1f)) {
                        Text(if (q.state == "hidden") "??? (undiscovered)" else q.name, fontSize = 13.sp)
                        if (q.state != "hidden") Text("${q.place} · ${q.difficulty} · ~${q.effortMin.toInt()} min${if (q.state == "progress") " · ${(q.progress * 100).toInt()}%" else ""}", fontSize = 10.sp)
                    }
                    Text(q.state, fontSize = 10.sp)
                }
            }
        }
        if (m.log.isNotEmpty()) Text(m.log.take(3).joinToString("\n"), fontSize = 11.sp, color = MaterialTheme.colorScheme.primary)
    }
}

/** The saved games: open one to start (or resume) tracking, or delete it (its recorded data is kept for diagnosis). */
@Composable
private fun GamesList(m: AppModel) {
    Text("Continue a game", style = MaterialTheme.typography.titleMedium)
    if (m.games.isEmpty()) Text("No saved games yet.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    m.games.forEach { g ->
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Text(g.name)
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Button(onClick = { m.openGame(g.id) }) { Text("Open", fontSize = 12.sp) }
                OutlinedButton(onClick = { m.deleteGame(g.id) }) { Text("Delete", fontSize = 12.sp) }
            }
        }
    }
}
