package dev.apgo2

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.CampText
import dev.apgo2.ui.IconLabel
import dev.apgo2.ui.MapOverlayCard
import dev.apgo2.ui.SavedBadge
import dev.apgo2.ui.circleExtremes
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.RealmOut

private const val OVERLAY_TOP_DP = 16
private const val OVERLAY_BOTTOM_DP = 150
private const val OVERLAY_BOTTOM_NAMED_DP = 230
private const val MIN_FRAME_POINTS = 2

// What the camp picker is showing and what the player has done: the pin, whether it is saved, and the camera requests.
@Stable
private class CampPickerState(
    private val m: AppModel,
    campId: String?,
    val askName: Boolean,
) {
    private val existing = m.camps.firstOrNull { it.id == campId }
    private val start: LatLng? =
        existing?.let { LatLng(it.lat, it.lon) } ?: m.here
            ?: m.realmOps.shown.firstOrNull()?.let { r ->
                r.circle?.let { LatLng(it.center.lat, it.center.lon) } ?: r.polygon.firstOrNull()?.let { LatLng(it.lat, it.lon) }
            }
    var pin by mutableStateOf(start)
    var name by mutableStateOf(existing?.name ?: m.campOps.defaultName())

    // The id of the camp being edited, or of the one this picker created.
    private var savedId by mutableStateOf(existing?.id)
    var saved by mutableStateOf(existing != null)
    var focus by mutableStateOf<MapFocus?>(null)
    private var nonce by mutableIntStateOf(0)

    // The camps drawn besides the pin: all the others.
    val others: List<LatLng> get() = m.camps.filter { it.id != savedId }.map { LatLng(it.lat, it.lon) }

    // Open showing your realms and the pin together, so the pin can be judged against the places you play.
    val framing: MapFit? =
        (m.realmOps.shown.flatMap(::realmPoints) + m.camps.map { LatLng(it.lat, it.lon) } + listOfNotNull(start)).let { pts ->
            if (pts.size >= MIN_FRAME_POINTS) MapFit(pts, 1) else null
        }

    val doneLabel get() = if (askName) CampText.SAVE else CampText.NEXT
    val canFinish get() = if (askName) pin != null else saved
    val hint get() = if (pin == null) CampText.TAP_TO_PLACE else CampText.DRAG_TO_MOVE

    // Without a name field every placement is saved at once; with one, [save] saves.
    fun place(to: LatLng) {
        pin = to
        if (!askName) save()
    }

    fun save() {
        val at = pin ?: return
        savedId = m.campOps.save(savedId, name, at)
        saved = savedId != null
    }

    // The top-right button: with a name field it saves first, then both go on once the camp is saved.
    fun finish(onNext: () -> Unit) {
        if (askName) save()
        if (saved) onNext()
    }

    fun useMyLocation() {
        m.here?.let {
            place(it)
            focus = MapFocus(it, ++nonce)
        }
    }

    // The points that bound a realm: its corners, or the four ends of its circle.
    private fun realmPoints(r: RealmOut): List<LatLng> =
        if (r.polygonActive) {
            r.polygon.map { LatLng(it.lat, it.lon) }
        } else {
            r.circle?.let { circleExtremes(LatLng(it.center.lat, it.center.lon), it.radiusM) }.orEmpty()
        }
}

/**
 * Choose where a camp is on a map. The pin can be dragged, the map tapped to put it there, or "My location" pressed. With [askName]
 * a name field shows and "Save camp" saves; without it (the setup wizard's first camp) each placement is saved at once and Next
 * stays disabled until the camp is saved. [campId] is the camp to edit, or null for a new one.
 */
@Composable
internal fun CampPicker(
    m: AppModel,
    title: String,
    campId: String?,
    askName: Boolean,
    onBack: () -> Unit,
    onNext: () -> Unit,
) {
    val s = remember { CampPickerState(m, campId, askName) }
    BackHandler { onBack() }
    Box(Modifier.fillMaxSize()) {
        QuestMap(
            emptyList(),
            m.realmOps.shown,
            emptyList(),
            { m.mePin },
            null,
            null,
            null,
            s::place,
            Modifier.fillMaxSize(),
            camps = s.others + listOfNotNull(s.pin),
            handles = listOfNotNull(s.pin),
            handlesVisible = false,
            onHandleMove = { _, to -> s.pin = to },
            onHandleRelease = { s.pin?.let(s::place) },
            focus = s.focus,
            fit = s.framing,
            overlayTopDp = OVERLAY_TOP_DP,
            overlayBottomDp = { if (askName) OVERLAY_BOTTOM_NAMED_DP else OVERLAY_BOTTOM_DP },
            lastPlace = m.lastPlace,
        )
        PickerButtons(s, m, title, onNext)
    }
}

// The save button and the "saved" badge (top right) and the card with the hint, name and "Use my location" (bottom).
@Composable
private fun BoxScope.PickerButtons(
    s: CampPickerState,
    m: AppModel,
    title: String,
    onNext: () -> Unit,
) {
    val topEnd = Modifier.windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.End))
    Button(
        onClick = { s.finish(onNext) },
        enabled = s.canFinish,
        modifier = Modifier.align(Alignment.TopEnd).then(topEnd).padding(top = 12.dp, end = 12.dp),
        contentPadding = PaddingValues(horizontal = 14.dp, vertical = 8.dp),
    ) {
        IconLabel(s.doneLabel, ApgoIcons.Done, textSize = 14.sp)
    }
    if (s.saved && !s.askName) {
        SavedBadge(CampText.SAVED, Modifier.align(Alignment.TopEnd).then(topEnd).padding(top = 72.dp, end = 16.dp))
    }
    MapOverlayCard(Modifier.align(Alignment.BottomCenter)) {
        Text(title, style = MaterialTheme.typography.titleSmall)
        Text(s.hint, style = MaterialTheme.typography.bodyMedium)
        if (s.askName) {
            OutlinedTextField(s.name, { s.name = it }, Modifier.fillMaxWidth(), label = { Text(CampText.NAME_LABEL) }, singleLine = true)
        }
        Button(onClick = s::useMyLocation, enabled = m.here != null, modifier = Modifier.fillMaxWidth()) {
            IconLabel(if (m.here == null) CampText.WAITING_FOR_LOCATION else CampText.USE_MY_LOCATION, ApgoIcons.Me, textSize = 14.sp)
        }
    }
}
