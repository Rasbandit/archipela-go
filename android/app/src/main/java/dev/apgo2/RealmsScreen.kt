package dev.apgo2

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.presence.needsAttention
import dev.apgo2.presence.nextStep
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.FeedbackText
import dev.apgo2.ui.HOME_PREVIEW_ZOOM
import dev.apgo2.ui.HomePreview
import dev.apgo2.ui.IconLabel
import dev.apgo2.ui.PREVIEW_DP
import dev.apgo2.ui.PreviewDot
import dev.apgo2.ui.PreviewFrame
import dev.apgo2.ui.RealmPreview
import dev.apgo2.ui.SetupText
import dev.apgo2.ui.Tone
import dev.apgo2.ui.circleRing
import dev.apgo2.ui.frameFor
import dev.apgo2.ui.mapSnapshot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.RealmOut

private const val DOT_LIMIT = 90u

/** Two views: the list of realms, and a full-page map editor for a new one. */
@Composable
internal fun RealmsScreen(m: AppModel) {
    m.editing?.let { id -> RealmEditor(m, id.ifEmpty { null }) { m.editing = null } }
        ?: RealmList(m, onNew = { m.editing = "" }, onEdit = { m.editing = it })
}

@Composable
private fun RealmList(
    m: AppModel,
    onNew: () -> Unit,
    onEdit: (String) -> Unit,
) {
    val snackbar = remember { SnackbarHostState() }
    var asking by remember { mutableStateOf<RealmOut?>(null) }
    ConfirmDelete(asking, onConfirm = { r ->
        asking = null
        m.realmOps.deleteWithUndo(r.id)
    }, onDismiss = { asking = null })
    UndoBar(m, snackbar)
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize().padding(start = 8.dp, end = 8.dp, top = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            HomeCard(m)
            Row(
                Modifier.fillMaxWidth().padding(top = 4.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Realms", style = MaterialTheme.typography.titleMedium)
                Button(onClick = onNew) { IconLabel("New realm", ApgoIcons.Add) }
            }
            if (m.realmOps.shown.isEmpty()) Text("No realms yet. Tap New realm to draw one.", fontSize = 13.sp)
            LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(m.realmOps.shown, key = { it.id }) { r ->
                    SwipeToDelete(onAsk = { asking = r }) { RealmCard(m, r) { onEdit(r.id) } }
                }
            }
        }
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter))
    }
}

// Show an Undo bar for each delete; when it goes away without Undo (or the screen is left) the realm is really deleted.
@Composable
private fun UndoBar(
    m: AppModel,
    snackbar: SnackbarHostState,
) {
    LaunchedEffect(m.realmOps.undo) {
        val u = m.realmOps.undo ?: return@LaunchedEffect
        var undone = false
        try {
            val result = snackbar.showSnackbar("Deleted ${u.name}", actionLabel = "Undo", duration = SnackbarDuration.Short)
            undone = result == SnackbarResult.ActionPerformed
        } finally {
            if (undone) m.realmOps.undoDelete(u.id) else m.realmOps.commitDelete(u.id)
        }
    }
}

// Swiping a row to the left asks to delete it; the row itself never goes away until the player confirms.
@Composable
private fun SwipeToDelete(
    onAsk: () -> Unit,
    content: @Composable () -> Unit,
) {
    val dismiss = rememberSwipeToDismissBoxState()
    val ask by rememberUpdatedState(onAsk)
    LaunchedEffect(dismiss.currentValue) {
        if (dismiss.currentValue == SwipeToDismissBoxValue.EndToStart) {
            ask()
            dismiss.reset()
        }
    }
    SwipeToDismissBox(
        state = dismiss,
        enableDismissFromStartToEnd = false,
        backgroundContent = {
            Box(
                Modifier
                    .fillMaxSize()
                    .clip(RoundedCornerShape(12.dp))
                    .background(ApgoPalette.danger)
                    .padding(end = 20.dp),
                contentAlignment = Alignment.CenterEnd,
            ) {
                Icon(ApgoIcons.Delete, contentDescription = "Delete", tint = ApgoPalette.onBrand)
            }
        },
    ) { content() }
}

// Home Base: where distances are measured from, plus the home Wi-Fi and car that pause the game. It looks different from a realm
// on purpose (a green outline and a house), so it is never mistaken for one. Tapping it opens the setup flow, at the first missing
// step when something is missing.
@Composable
private fun HomeCard(m: AppModel) {
    val context = LocalContext.current
    val home = m.home?.let { LatLng(it.lat, it.lon) }
    val progress = m.setup.progress()
    val map by produceState<android.graphics.Bitmap?>(null, home) {
        value = null
        value = home?.let { runCatching { mapSnapshot(context, PreviewFrame(it, HOME_PREVIEW_ZOOM)) }.getOrNull() }
    }
    Card(
        Modifier.fillMaxWidth().clickable { m.setup.open(if (progress.needsAttention()) progress.nextStep() else null) },
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(2.dp, ApgoPalette.home),
    ) {
        PreviewRow(preview = { if (home != null) HomePreview(map, Modifier.size(PREVIEW_DP.dp)) }) {
            CardTitle(ApgoIcons.Home, ApgoPalette.home, SetupText.HOME_BASE_NAME)
            Text(if (home == null) SetupText.HOME_BASE_UNSET else SetupText.HOME_BASE_CARD, style = MaterialTheme.typography.bodyMedium)
            if (progress.missingWifi) FeedbackText(SetupText.HOME_NEEDS_WIFI, Tone.Warning)
        }
    }
}

// A realm at a glance: its icon and name, how many finds and quest types it offers, and a small preview of the region.
@Composable
private fun RealmCard(
    m: AppModel,
    r: RealmOut,
    onClick: () -> Unit,
) {
    val types = m.offers[r.id].orEmpty().size
    val dots by produceState(emptyList<PreviewDot>(), r.id, r.scannedAtMs, types) {
        value =
            if (r.scannedAtMs == null) {
                emptyList()
            } else {
                withContext(Dispatchers.IO) {
                    m.engine.realmDots(r.id, DOT_LIMIT).map { PreviewDot(it.at.lat, it.at.lon, ApgoPalette.kind(it.kindId, it.family)) }
                }
            }
    }
    val outline =
        if (r.polygonActive) {
            r.polygon.map { it.lat to it.lon }
        } else {
            r.circle?.let { circleRing(it.center.lat, it.center.lon, it.radiusM) }.orEmpty()
        }
    val context = LocalContext.current
    val frame = frameFor(outline)
    // The picture belongs to one frame: when the shape changes it is dropped at once (never shown under a different outline) and
    // drawn again.
    val map by produceState<android.graphics.Bitmap?>(null, frame?.key) {
        value = null
        value = frame?.let { runCatching { mapSnapshot(context, it) }.getOrNull() }
    }
    Card(Modifier.fillMaxWidth().clickable(onClick = onClick)) {
        PreviewRow(preview = { RealmPreview(outline, dots, map, Modifier.size(PREVIEW_DP.dp)) }) {
            CardTitle(ApgoIcons.realm(r.icon), MaterialTheme.colorScheme.primary, r.name, singleLine = true)
            if (r.scannedAtMs == null) {
                Text("Not scanned yet", fontSize = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Text("${r.places} finds", style = MaterialTheme.typography.bodyLarge)
                Text("$types quest types", fontSize = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

// The layout both cards share: text on the left, a square preview on the right.
@Composable
private fun PreviewRow(
    preview: @Composable () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp), content = content)
        preview()
    }
}

@Composable
private fun CardTitle(
    icon: ImageVector,
    tint: Color,
    title: String,
    singleLine: Boolean = false,
) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(26.dp))
        if (singleLine) {
            Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        } else {
            Text(title, style = MaterialTheme.typography.titleMedium)
        }
    }
}

// Asks before a realm is deleted (swipe or editor). [realm] null shows nothing.
@Composable
internal fun ConfirmDelete(
    realm: RealmOut?,
    onConfirm: (RealmOut) -> Unit,
    onDismiss: () -> Unit,
) {
    val r = realm ?: return
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Delete ${r.name}?") },
        text = { Text("Its finds and your favorites and bans go with it. Games already started keep their quests.") },
        confirmButton = { TextButton(onClick = { onConfirm(r) }) { Text("Delete", color = ApgoPalette.danger) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
