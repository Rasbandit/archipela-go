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
            item { Text(RecapFormat.subtitle(r)) }
            if (r.route.isNotEmpty()) item { RouteMap(r.route) }
            items(RecapFormat.sections(r), key = { it.first }) { (label, lines) ->
                Text(label, style = MaterialTheme.typography.titleSmall)
                lines.forEach { Text(it) }
            }
            item { Button(onClick = onDone, modifier = Modifier.fillMaxWidth()) { Text(Help.RECAP_DONE) } }
        }
    }
}
