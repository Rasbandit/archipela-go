package dev.apgo2

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import dev.apgo2.ui.Help
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.MapLibreMapOptions
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.sources.GeoJsonSource
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.RecapOut

/** "Welcome home": how the last walk went, shown over the app until dismissed. */
@Composable
internal fun RecapScreen(
    r: RecapOut,
    onDone: () -> Unit,
) {
    BackHandler(onBack = onDone)
    Surface(Modifier.fillMaxSize()) {
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Text(Help.RECAP_TITLE, style = MaterialTheme.typography.headlineMedium) }
            item {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    RecapFormat.stats(r).forEach { (label, value) -> StatTile(label, value) }
                }
            }
            if (r.route.isNotEmpty()) item { RouteMap(r.route) }
            items(RecapFormat.sections(r), key = { it.first }) { (label, lines) ->
                Text(label, style = MaterialTheme.typography.titleSmall)
                lines.forEach { Text(it) }
            }
            item { Button(onClick = onDone, modifier = Modifier.fillMaxWidth()) { Text(Help.RECAP_DONE) } }
        }
    }
}

@Composable
private fun StatTile(
    label: String,
    value: String,
) {
    Card(Modifier.semantics(mergeDescendants = true) {}) {
        Text(value, Modifier.padding(start = 12.dp, top = 8.dp, end = 12.dp), style = MaterialTheme.typography.titleLarge)
        Text(label, Modifier.padding(start = 12.dp, end = 12.dp, bottom = 8.dp), style = MaterialTheme.typography.labelMedium)
    }
}

private const val MAP_HEIGHT_DP = 200
private const val MAP_CORNER_DP = 12
private const val MAP_PADDING_PX = 48

// The walk on a still map: the route drawn by the map's own trace layer, the camera fitted to it. No gestures, so the list scrolls.
@Composable
private fun RouteMap(route: List<List<GeoPoint>>) {
    val context = LocalContext.current
    val view =
        remember {
            MapStyle.init(context)
            val options = MapLibreMapOptions.createFromAttributes(context).logoEnabled(false).attributionEnabled(false)
            MapView(context, options).also { v ->
                v.getMapAsync { map ->
                    map.uiSettings.setAllGesturesEnabled(false)
                    map.setStyle(Style.Builder().fromUri(MapStyle.URL)) { style ->
                        MapStyle.install(style)
                        style.getSourceAs<GeoJsonSource>(MapSource.TRACE)?.setGeoJson(GeoJson.collection(RecapFeatures.route(route)))
                        val points = route.flatten().map { LatLng(it.lat, it.lon) }
                        if (points.size >= 2) {
                            map.moveCamera(CameraUpdateFactory.newLatLngBounds(LatLngBounds.fromLatLngs(points), MAP_PADDING_PX))
                        }
                    }
                }
            }
        }
    MapLifecycle(view, onShow = true)
    Box(Modifier.fillMaxWidth().height(MAP_HEIGHT_DP.dp).clip(RoundedCornerShape(MAP_CORNER_DP.dp))) {
        AndroidView({ view }, Modifier.fillMaxSize())
        Text(
            MapStyle.CREDIT,
            Modifier.align(Alignment.BottomEnd).background(MaterialTheme.colorScheme.surface).padding(horizontal = 4.dp),
            style = MaterialTheme.typography.labelSmall,
        )
    }
}
