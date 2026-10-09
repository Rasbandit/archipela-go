package dev.apgo2

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import dev.apgo2.ui.IconLabel
import dev.apgo2.ui.MapOverlayCard
import dev.apgo2.ui.SavedBadge
import dev.apgo2.ui.circleExtremes
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.RealmOut

private const val OVERLAY_TOP_DP = 16
private const val OVERLAY_BOTTOM_DP = 150
private const val MIN_FRAME_POINTS = 2

// What the home picker is showing and what the player has done: the pin, whether it is saved, and the camera requests.
@Stable
private class HomePickerState(
    private val m: AppModel,
) {
    private val start: LatLng? =
        m.home?.let { LatLng(it.lat, it.lon) } ?: m.me
            ?: m.realmOps.shown.firstOrNull()?.let { r ->
                r.circle?.let { LatLng(it.center.lat, it.center.lon) } ?: r.polygon.firstOrNull()?.let { LatLng(it.lat, it.lon) }
            }
    var pin by mutableStateOf(start)
    var saved by mutableStateOf(m.home != null)
    var focus by mutableStateOf<MapFocus?>(null)
    private var nonce by mutableIntStateOf(0)

    // Open showing your realms and the pin together, so the pin can be judged against the places you play.
    val framing: MapFit? =
        (m.realmOps.shown.flatMap(::realmPoints) + listOfNotNull(start)).let { pts ->
            if (pts.size >= MIN_FRAME_POINTS) MapFit(pts, 1) else null
        }

    fun place(to: LatLng) {
        pin = to
        m.realmOps.setHome(to, announce = false)
        saved = true
    }

    fun useMyLocation() {
        m.me?.let {
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
 * Setup step 1: choose home on a map. The pin can be dragged, the map tapped to put it there, or "My location" pressed. Each
 * placement is saved at once; Next stays disabled until home is saved.
 */
@Composable
internal fun HomePicker(
    m: AppModel,
    title: String,
    onBack: () -> Unit,
    onNext: () -> Unit,
) {
    val s = remember { HomePickerState(m) }
    BackHandler { onBack() }
    val topEnd = Modifier.windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.End))
    Box(Modifier.fillMaxSize()) {
        QuestMap(
            emptyList(),
            m.realmOps.shown,
            emptyList(),
            m.me,
            null,
            null,
            null,
            s::place,
            Modifier.fillMaxSize(),
            home = s.pin,
            handles = listOfNotNull(s.pin),
            handlesVisible = false,
            onHandleMove = { _, to -> s.pin = to },
            onHandleRelease = { s.pin?.let(s::place) },
            focus = s.focus,
            fit = s.framing,
            overlayTopDp = OVERLAY_TOP_DP,
            overlayBottomDp = OVERLAY_BOTTOM_DP,
            lastPlace = m.lastPlace,
        )
        Row(
            Modifier.align(Alignment.TopEnd).then(topEnd).padding(top = 12.dp, end = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Button(onClick = onNext, enabled = s.saved, contentPadding = PaddingValues(horizontal = 14.dp, vertical = 8.dp)) {
                IconLabel("Next", ApgoIcons.Done, textSize = 14.sp)
            }
        }
        if (s.saved) SavedBadge("Home saved", Modifier.align(Alignment.TopEnd).then(topEnd).padding(top = 72.dp, end = 16.dp))
        MapOverlayCard(Modifier.align(Alignment.BottomCenter)) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            Text(
                if (s.pin == null) {
                    "Tap the map to put your home there."
                } else {
                    "Drag the pin or tap the map to move it. Distances in your games are measured from here."
                },
                style = MaterialTheme.typography.bodyMedium,
            )
            Button(onClick = s::useMyLocation, enabled = m.me != null, modifier = Modifier.fillMaxWidth()) {
                IconLabel(if (m.me == null) "Waiting for your location…" else "Use my location", ApgoIcons.Me, textSize = 14.sp)
            }
        }
    }
}
