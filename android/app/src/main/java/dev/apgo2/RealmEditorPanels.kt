package dev.apgo2

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.IconChoices
import dev.apgo2.ui.MapBubble
import dev.apgo2.ui.MapOverlayCard
import dev.apgo2.ui.MarkToggle
import dev.apgo2.ui.RealmStatsBox
import dev.apgo2.ui.ScanFigures
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.apgo_ffi.CircleOut
import uniffi.apgo_ffi.FindOut
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.RealmStatsOut

// Items above the finds in the Details list: name + mode, search + filters, count.
internal const val HEADER_ITEMS = 3

private const val MIN_CORNERS = 3
private const val METERS_PER_KM = 1000
private const val DETAILS_PANEL_FRACTION = 0.5f
private const val BANNED_ALPHA = 0.5f
private const val MAX_BUBBLE_KINDS = 2

private fun distanceLabel(m: Double) = if (m < METERS_PER_KM) "${m.toInt()} m" else "%.1f km".format(m / METERS_PER_KM)

// Area is just the map and a box of numbers about what is chosen.
@Composable
internal fun BoxScope.AreaPanel(s: RealmEditorState) {
    MapOverlayCard(Modifier.align(Alignment.BottomCenter).onSizeChanged { s.panelPx = it.height }) {
        if (s.polygon && s.m.draft.size < MIN_CORNERS) {
            Text(
                "Tap the map to add corners (${s.m.draft.size} of at least $MIN_CORNERS).",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            ShapeStats(s)
        }
    }
}

// The size of the shape, and the figures from the last scan.
@Composable
private fun ShapeStats(s: RealmEditorState) {
    val m = s.m
    val circleOut = s.circleCenter?.let { CircleOut(GeoPoint(it.latitude, it.longitude), s.radius.toDouble()) }
    val corners = m.draft.toList()
    val shape =
        remember(s.polygon, s.radius, s.circleCenter, corners, m.home) {
            m.engine.shapeStats(
                circleOut,
                corners.map { GeoPoint(it.latitude, it.longitude) },
                s.polygon && corners.size >= MIN_CORNERS,
                m.home,
            )
        }
    // Figures from the last scan. After the outline changes they are the old ones, marked as such until Details refreshes them.
    var found by remember(s.id) { mutableStateOf<RealmStatsOut?>(null) }
    LaunchedEffect(s.id, s.current?.scannedAtMs) {
        val rid = s.id
        found = if (rid != null && s.current?.scannedAtMs != null) withContext(Dispatchers.IO) { m.engine.realmStats(rid) } else null
    }
    val waiting = if (m.busy != null) "looking…" else "after scan"
    RealmStatsBox(shape.areaM2, shape.farthestM, found?.let { it.toFigures(stale = s.shapeDirty) }, waiting)
}

private fun RealmStatsOut.toFigures(stale: Boolean) =
    ScanFigures(walkableM, streets.toInt(), trailM, finds.toInt(), parks.toInt(), roughShare, stale = stale)

// Details: the realm's name and icon, a search over its finds, and the finds themselves.
@Composable
internal fun BoxScope.DetailsPanel(
    s: RealmEditorState,
    view: FindsView,
    listState: LazyListState,
) {
    MapOverlayCard(
        Modifier.align(Alignment.BottomCenter).onSizeChanged { s.panelPx = it.height }.fillMaxHeight(DETAILS_PANEL_FRACTION),
        fillHeight = true,
    ) {
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), state = listState) {
            headerItems(s, view)
            items(view.shown, key = { it.id }) { f ->
                FindRow(s, f)
                HorizontalDivider()
            }
        }
    }
}

// A compact header so the finds get most of the half-height panel: icon + name, search + filters, then the count.
private fun LazyListScope.headerItems(
    s: RealmEditorState,
    view: FindsView,
) {
    item(key = "name") { NameRow(s) }
    item(key = "search") { SearchRow(s) }
    item(key = "count") {
        Text(
            findsSummary(s, view),
            Modifier.padding(vertical = 4.dp),
            fontSize = 11.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        s.current?.let { r ->
            TextButton(onClick = { s.confirmDelete = r }) {
                Icon(ApgoIcons.Delete, contentDescription = null, tint = ApgoPalette.danger, modifier = Modifier.size(16.dp))
                Text("  Delete realm", fontSize = 12.sp, color = ApgoPalette.danger)
            }
        }
    }
}

private fun findsSummary(
    s: RealmEditorState,
    view: FindsView,
): String =
    when {
        s.m.busy != null -> {
            "Looking for finds…"
        }

        s.current?.scannedAtMs == null -> {
            "No finds yet."
        }

        s.findsVersion == 0 -> {
            "Loading finds…"
        }

        else -> {
            val favorites = view.visible.count { it.mark == FindFilter.FAVORITE }
            val banned = view.visible.count { it.mark == FindFilter.BANNED }
            "${view.shown.size} of ${view.visible.size} finds · $favorites favorites · $banned banned"
        }
    }

@Composable
private fun NameRow(s: RealmEditorState) {
    Row(
        Modifier.fillMaxWidth().padding(top = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        IconButton(onClick = { s.pickingIcon = true }) {
            Icon(
                ApgoIcons.realm(s.icon),
                contentDescription = "Choose an icon",
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(28.dp),
            )
        }
        OutlinedTextField(s.name, { s.name = it }, label = { Text("Realm name") }, singleLine = true, modifier = Modifier.weight(1f))
    }
}

@Composable
private fun SearchRow(s: RealmEditorState) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        OutlinedTextField(s.query, { s.query = it }, label = { Text("Search finds") }, singleLine = true, modifier = Modifier.weight(1f))
        IconChoices(
            listOf(FindFilter.ALL, FindFilter.FAVORITE, FindFilter.BANNED),
            s.filter,
            { s.filter = it },
            {
                when (it) {
                    FindFilter.FAVORITE -> ApgoIcons.Favorite
                    FindFilter.BANNED -> ApgoIcons.Banned
                    else -> ApgoIcons.All
                }
            },
            {
                when (it) {
                    FindFilter.ALL -> "Show all finds"
                    FindFilter.FAVORITE -> "Show favorites"
                    else -> "Show banned"
                }
            },
        )
    }
}

// One find in the Details list: its icon, name and kinds, and the Favorite and Ban toggles.
@Composable
private fun FindRow(
    s: RealmEditorState,
    f: FindOut,
) {
    val density = LocalDensity.current.density
    val banned = f.mark == FindFilter.BANNED
    val background = if (f.id == s.selectedFind) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent
    Row(Modifier.fillMaxWidth().background(background).clickable { s.show(f, density) }, verticalAlignment = Alignment.CenterVertically) {
        Icon(
            ApgoIcons.forKind(f.kindId, f.family),
            contentDescription = null,
            modifier = Modifier.padding(horizontal = 8.dp).size(22.dp),
            tint = if (banned) ApgoPalette.muted else ApgoPalette.kind(f.kindId, f.family),
        )
        Column(Modifier.weight(1f).alpha(if (banned) BANNED_ALPHA else 1f)) {
            Text(
                f.name,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodyMedium,
                textDecoration = if (banned) TextDecoration.LineThrough else null,
            )
            Text(
                findSubtitle(f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        MarkToggle(
            ApgoIcons.Favorite,
            "Favorite",
            f.mark == FindFilter.FAVORITE,
            ApgoPalette.favorite,
            onClick = { s.mark(f, FindFilter.FAVORITE) },
        )
        MarkToggle(ApgoIcons.Banned, "Ban", banned, ApgoPalette.banned, onClick = { s.mark(f, FindFilter.BANNED) })
    }
}

// An unnamed find is titled by its first quest kind, so the subtitle must not repeat it.
private fun findSubtitle(f: FindOut): String {
    val kinds = if (f.named) f.kinds.map { it.name } else listOf("unnamed") + f.kinds.drop(1).map { it.name }
    return kinds.joinToString(", ") + " · ${distanceLabel(f.distanceM)}"
}

// The dialog to pick the realm's icon.
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun IconPickerDialog(s: RealmEditorState) {
    if (!s.pickingIcon) return
    AlertDialog(
        onDismissRequest = { s.pickingIcon = false },
        title = { Text("Choose an icon") },
        text = {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                ApgoIcons.realmChoices.forEach { (key, vector) ->
                    val chosen = key == (s.icon ?: "pin")
                    IconButton(
                        onClick = { s.pickIcon(key) },
                        colors =
                            IconButtonDefaults.iconButtonColors(
                                containerColor = if (chosen) MaterialTheme.colorScheme.primary else Color.Transparent,
                                contentColor = if (chosen) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                            ),
                    ) { Icon(vector, contentDescription = key) }
                }
            }
        },
        confirmButton = { TextButton(onClick = { s.pickingIcon = false }) { Text("Done") } },
    )
}

// A callout over the map for the selected find: what it is, what the quests mean, and how to complete them.
@Composable
internal fun FindBubble(
    f: FindOut,
    at: Offset,
    onSize: (IntSize) -> Unit,
    onMark: (String) -> Unit,
    onClose: () -> Unit,
) {
    MapBubble(at, onSize) {
        BubbleHeader(f, onMark, onClose)
        f.kinds.take(MAX_BUBBLE_KINDS).forEach { k ->
            Column(Modifier.padding(end = 8.dp)) {
                Text(k.name, style = MaterialTheme.typography.labelLarge, color = ApgoPalette.kind(k.id, k.family))
                Text("${k.blurb} ${k.how}", fontSize = 12.sp, lineHeight = 16.sp)
            }
        }
        if (f.kinds.size > MAX_BUBBLE_KINDS) {
            val more = f.kinds.size - MAX_BUBBLE_KINDS
            Text("+ $more more quest types", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (f.tags.isNotEmpty()) {
            Text(
                "Mapped as ${f.tags.joinToString(" · ")}",
                fontSize = 10.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(end = 8.dp),
            )
        }
    }
}

@Composable
private fun BubbleHeader(
    f: FindOut,
    onMark: (String) -> Unit,
    onClose: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(
            ApgoIcons.forKind(f.kindId, f.family),
            contentDescription = null,
            tint = ApgoPalette.kind(f.kindId, f.family),
            modifier = Modifier.size(24.dp),
        )
        Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
            Text(f.name, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(
                "${distanceLabel(f.distanceM)} from home" + if (f.named) "" else " · unnamed",
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        MarkToggle(
            ApgoIcons.Favorite,
            "Favorite",
            f.mark == FindFilter.FAVORITE,
            ApgoPalette.favorite,
            onClick = { onMark(FindFilter.FAVORITE) },
        )
        MarkToggle(ApgoIcons.Banned, "Ban", f.mark == FindFilter.BANNED, ApgoPalette.banned, onClick = { onMark(FindFilter.BANNED) })
        IconButton(onClick = onClose) { Icon(ApgoIcons.Close, contentDescription = "Close") }
    }
}
