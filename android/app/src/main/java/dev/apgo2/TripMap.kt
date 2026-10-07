package dev.apgo2

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import org.json.JSONArray
import org.json.JSONObject
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.PropertyFactory.circleColor
import org.maplibre.android.style.layers.PropertyFactory.circleRadius
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeColor
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeWidth
import org.maplibre.android.style.sources.GeoJsonSource

/** A generated real-world point for one apworld trip. `state`: open | locked | done. */
data class GameTrip(
    val locationId: Long,
    val tier: Int,
    val keyNeeded: Int,
    val name: String,
    val lat: Double,
    val lon: Double,
    val distanceM: Double,
    val state: String,
)

private const val STYLE_URL = "https://tiles.openfreemap.org/styles/liberty"

private fun tripsGeoJson(trips: List<GameTrip>): String {
    val features = JSONArray()
    trips.forEach { t ->
        features.put(
            JSONObject()
                .put("type", "Feature")
                .put("geometry", JSONObject().put("type", "Point").put("coordinates", JSONArray().put(t.lon).put(t.lat)))
                .put("properties", JSONObject().put("state", t.state).put("name", t.name).put("tier", t.tier)),
        )
    }
    return JSONObject().put("type", "FeatureCollection").put("features", features).toString()
}

private fun mePoint(me: LatLng?): String {
    val features = JSONArray()
    me?.let {
        features.put(
            JSONObject().put("type", "Feature").put("properties", JSONObject())
                .put("geometry", JSONObject().put("type", "Point").put("coordinates", JSONArray().put(it.longitude).put(it.latitude))),
        )
    }
    return JSONObject().put("type", "FeatureCollection").put("features", features).toString()
}

@Composable
fun TripMap(trips: List<GameTrip>, me: LatLng?, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val mapView = remember {
        MapLibre.getInstance(context)
        MapView(context)
    }
    var style by remember { mutableStateOf<Style?>(null) }
    var map by remember { mutableStateOf<MapLibreMap?>(null) }
    var centered by remember { mutableStateOf(false) }

    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle, mapView) {
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_CREATE -> mapView.onCreate(null)
                Lifecycle.Event.ON_START -> mapView.onStart()
                Lifecycle.Event.ON_RESUME -> mapView.onResume()
                Lifecycle.Event.ON_PAUSE -> mapView.onPause()
                Lifecycle.Event.ON_STOP -> mapView.onStop()
                Lifecycle.Event.ON_DESTROY -> mapView.onDestroy()
                else -> {}
            }
        }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }

    LaunchedEffect(mapView) {
        mapView.getMapAsync { m ->
            map = m
            m.setStyle(Style.Builder().fromUri(STYLE_URL)) { s ->
                s.addSource(GeoJsonSource("trips", tripsGeoJson(emptyList())))
                s.addLayer(
                    CircleLayer("trips-circles", "trips").withProperties(
                        circleRadius(8f),
                        circleColor(
                            Expression.match(
                                Expression.get("state"),
                                Expression.literal("#d32f2f"),
                                Expression.stop("locked", Expression.literal("#9e9e9e")),
                                Expression.stop("done", Expression.literal("#2e7d32")),
                            ),
                        ),
                        circleStrokeColor("#ffffff"),
                        circleStrokeWidth(1.5f),
                    ),
                )
                s.addSource(GeoJsonSource("me", mePoint(null)))
                s.addLayer(
                    CircleLayer("me-circle", "me").withProperties(
                        circleRadius(9f),
                        circleColor("#1565c0"),
                        circleStrokeColor("#ffffff"),
                        circleStrokeWidth(3f),
                    ),
                )
                style = s
            }
        }
    }

    LaunchedEffect(style, trips) {
        style?.getSourceAs<GeoJsonSource>("trips")?.setGeoJson(tripsGeoJson(trips))
    }
    LaunchedEffect(style, me) {
        style?.getSourceAs<GeoJsonSource>("me")?.setGeoJson(mePoint(me))
        if (me != null && !centered) {
            map?.animateCamera(CameraUpdateFactory.newLatLngZoom(me, 13.0))
            centered = true
        }
    }

    AndroidView(factory = { mapView }, modifier = modifier)
}
