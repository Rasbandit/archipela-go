package dev.apgo2

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlin.math.cos
import kotlin.math.sin
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
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.PropertyFactory.circleColor
import org.maplibre.android.style.layers.PropertyFactory.circleRadius
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeColor
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeWidth
import org.maplibre.android.style.layers.PropertyFactory.fillColor
import org.maplibre.android.style.layers.PropertyFactory.fillOpacity
import org.maplibre.android.style.layers.PropertyFactory.lineColor
import org.maplibre.android.style.layers.PropertyFactory.lineWidth
import org.maplibre.android.style.sources.GeoJsonSource
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.RealmOut

private const val STYLE_URL = "https://tiles.openfreemap.org/styles/liberty"

private fun coord(lat: Double, lon: Double) = JSONArray().put(lon).put(lat)

private fun feature(geometry: JSONObject, props: JSONObject = JSONObject()) =
    JSONObject().put("type", "Feature").put("geometry", geometry).put("properties", props)

private fun pointGeo(lat: Double, lon: Double) = JSONObject().put("type", "Point").put("coordinates", coord(lat, lon))

private fun fc(features: List<JSONObject>) = JSONObject().put("type", "FeatureCollection").put("features", JSONArray(features)).toString()

private fun ring(points: List<Pair<Double, Double>>): JSONArray {
    val r = JSONArray()
    points.forEach { r.put(coord(it.first, it.second)) }
    points.firstOrNull()?.let { r.put(coord(it.first, it.second)) }
    return r
}

private fun circleRing(lat: Double, lon: Double, radiusM: Double): List<Pair<Double, Double>> =
    (0 until 48).map { i ->
        val a = Math.toRadians(i * 7.5)
        val dLat = radiusM * cos(a) / 111_195.0
        val dLon = radiusM * sin(a) / (111_195.0 * cos(Math.toRadians(lat)))
        (lat + dLat) to (lon + dLon)
    }

private fun questFeatures(quests: List<QuestOut>, selected: Long?): List<JSONObject> =
    quests.filter { it.state != "hidden" && it.anchor != null && it.shape != "line" }.map {
        val a = it.anchor!!
        feature(
            pointGeo(a.lat, a.lon),
            JSONObject().put("state", it.state).put("diff", if (it.boss) "boss" else it.difficulty).put("sel", it.locationId == selected),
        )
    } + quests.filter { it.state != "hidden" && it.shape == "courier" && it.anchorB != null }.map {
        val b = it.anchorB!!
        feature(pointGeo(b.lat, b.lon), JSONObject().put("state", it.state).put("diff", "easy").put("sel", it.locationId == selected))
    }

private fun lineFeatures(quests: List<QuestOut>): List<JSONObject> =
    quests.filter { it.state != "hidden" && it.path.size >= 2 && (it.shape == "line" || it.shape == "area") }.map {
        val coords = JSONArray()
        it.path.forEach { p -> coords.put(coord(p.lat, p.lon)) }
        feature(JSONObject().put("type", "LineString").put("coordinates", coords), JSONObject().put("state", it.state))
    }

private fun areaFeatures(quests: List<QuestOut>): List<JSONObject> =
    quests.filter { it.state != "hidden" && it.shape == "area" && it.path.size >= 3 }.map {
        feature(
            JSONObject().put("type", "Polygon").put("coordinates", JSONArray().put(ring(it.path.map { p -> p.lat to p.lon }))),
            JSONObject().put("state", it.state),
        )
    }

private fun realmFeatures(realms: List<RealmOut>): List<JSONObject> = realms.mapNotNull { r ->
    val pts = r.circle?.let { circleRing(it.center.lat, it.center.lon, it.radiusM) } ?: r.polygon.takeIf { it.size >= 3 }?.map { it.lat to it.lon }
    pts?.let {
        feature(JSONObject().put("type", "Polygon").put("coordinates", JSONArray().put(ring(it))), JSONObject().put("name", r.name))
    }
}

private fun draftFeatures(draft: List<LatLng>): List<JSONObject> {
    val out = mutableListOf<JSONObject>()
    if (draft.size >= 3) out += feature(JSONObject().put("type", "Polygon").put("coordinates", JSONArray().put(ring(draft.map { it.latitude to it.longitude }))))
    else if (draft.size == 2) out += feature(JSONObject().put("type", "LineString").put("coordinates", JSONArray().put(coord(draft[0].latitude, draft[0].longitude)).put(coord(draft[1].latitude, draft[1].longitude))))
    draft.forEach { out += feature(pointGeo(it.latitude, it.longitude)) }
    return out
}

private fun stateColor() = Expression.match(
    Expression.get("state"),
    Expression.literal("#d32f2f"),
    Expression.stop("locked", Expression.literal("#9e9e9e")),
    Expression.stop("done", Expression.literal("#2e7d32")),
    Expression.stop("progress", Expression.literal("#f9a825")),
)

@Composable
fun QuestMap(
    quests: List<QuestOut>,
    realms: List<RealmOut>,
    draft: List<LatLng>,
    me: LatLng?,
    thaw: LatLng?,
    waypoint: LatLng?,
    selected: Long?,
    onMapClick: (LatLng) -> Unit,
    modifier: Modifier = Modifier,
    home: LatLng? = null,
    onMapLongClick: ((LatLng) -> Unit)? = null,
) {
    val context = LocalContext.current
    val mapView = remember {
        MapLibre.getInstance(context)
        MapView(context)
    }
    var style by remember { mutableStateOf<Style?>(null) }
    var map by remember { mutableStateOf<MapLibreMap?>(null) }
    var centered by remember { mutableStateOf(false) }
    val clickHandler by rememberUpdatedState(onMapClick)
    val longClickHandler by rememberUpdatedState(onMapLongClick)

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
            m.addOnMapClickListener { ll -> clickHandler(ll); true }
            m.addOnMapLongClickListener { ll -> longClickHandler?.invoke(ll) != null }
            m.setStyle(Style.Builder().fromUri(STYLE_URL)) { s ->
                val empty = fc(emptyList())
                listOf("realms", "areas", "lines", "quests", "draft", "marks", "home", "me").forEach { s.addSource(GeoJsonSource(it, empty)) }
                s.addLayer(FillLayer("realms-fill", "realms").withProperties(fillColor("#1565c0"), fillOpacity(0.07f)))
                s.addLayer(LineLayer("realms-line", "realms").withProperties(lineColor("#1565c0"), lineWidth(1.8f)))
                s.addLayer(FillLayer("areas-fill", "areas").withProperties(fillColor(stateColor()), fillOpacity(0.18f)))
                s.addLayer(LineLayer("lines-layer", "lines").withProperties(lineColor(stateColor()), lineWidth(4f)))
                s.addLayer(
                    CircleLayer("quests-sel", "quests").withFilter(Expression.eq(Expression.get("sel"), Expression.literal(true))).withProperties(
                        circleRadius(19f), circleColor("#ffffff"), circleStrokeColor("#1565c0"), circleStrokeWidth(3f),
                    ),
                )
                s.addLayer(
                    CircleLayer("quests-layer", "quests").withProperties(
                        circleRadius(
                            Expression.match(
                                Expression.get("diff"), Expression.literal(8f),
                                Expression.stop("easy", Expression.literal(6f)),
                                Expression.stop("medium", Expression.literal(9f)),
                                Expression.stop("hard", Expression.literal(12f)),
                                Expression.stop("boss", Expression.literal(16f)),
                            ),
                        ),
                        circleColor(stateColor()), circleStrokeColor("#ffffff"), circleStrokeWidth(1.5f),
                    ),
                )
                s.addLayer(LineLayer("draft-line", "draft").withProperties(lineColor("#ef6c00"), lineWidth(3f)))
                s.addLayer(FillLayer("draft-fill", "draft").withProperties(fillColor("#ef6c00"), fillOpacity(0.15f)))
                s.addLayer(CircleLayer("draft-pts", "draft").withProperties(circleRadius(5f), circleColor("#ef6c00"), circleStrokeColor("#ffffff"), circleStrokeWidth(1.5f)))
                s.addLayer(CircleLayer("marks-layer", "marks").withProperties(circleRadius(12f), circleColor(Expression.get("color")), circleStrokeColor("#ffffff"), circleStrokeWidth(3f)))
                s.addLayer(CircleLayer("home-ring", "home").withProperties(circleRadius(14f), circleColor("#2e7d32"), circleStrokeColor("#ffffff"), circleStrokeWidth(3f)))
                s.addLayer(CircleLayer("home-dot", "home").withProperties(circleRadius(5f), circleColor("#ffffff")))
                s.addLayer(CircleLayer("me-layer", "me").withProperties(circleRadius(9f), circleColor("#1565c0"), circleStrokeColor("#ffffff"), circleStrokeWidth(3f)))
                style = s
            }
        }
    }

    LaunchedEffect(style, realms) { style?.getSourceAs<GeoJsonSource>("realms")?.setGeoJson(fc(realmFeatures(realms))) }
    LaunchedEffect(style, quests, selected) {
        style?.getSourceAs<GeoJsonSource>("quests")?.setGeoJson(fc(questFeatures(quests, selected)))
        style?.getSourceAs<GeoJsonSource>("lines")?.setGeoJson(fc(lineFeatures(quests)))
        style?.getSourceAs<GeoJsonSource>("areas")?.setGeoJson(fc(areaFeatures(quests)))
    }
    LaunchedEffect(style, draft) { style?.getSourceAs<GeoJsonSource>("draft")?.setGeoJson(fc(draftFeatures(draft))) }
    LaunchedEffect(style, thaw, waypoint) {
        val marks = mutableListOf<JSONObject>()
        thaw?.let { marks += feature(pointGeo(it.latitude, it.longitude), JSONObject().put("color", "#00acc1")) }
        waypoint?.let { marks += feature(pointGeo(it.latitude, it.longitude), JSONObject().put("color", "#8e24aa")) }
        style?.getSourceAs<GeoJsonSource>("marks")?.setGeoJson(fc(marks))
    }
    LaunchedEffect(style, home) {
        style?.getSourceAs<GeoJsonSource>("home")?.setGeoJson(fc(home?.let { listOf(feature(pointGeo(it.latitude, it.longitude))) } ?: emptyList()))
    }
    LaunchedEffect(style, me) {
        style?.getSourceAs<GeoJsonSource>("me")?.setGeoJson(fc(me?.let { listOf(feature(pointGeo(it.latitude, it.longitude))) } ?: emptyList()))
    }
    // Frame the action once: all visible quests plus you, or just you. Done after layout so the camera move is not dropped.
    LaunchedEffect(style, me, quests.isNotEmpty(), realms.size) {
        val m = map ?: return@LaunchedEffect
        if (style == null || centered) return@LaunchedEffect
        val pts = quests.filter { it.state != "hidden" }.mapNotNull { q -> q.anchor?.let { LatLng(it.lat, it.lon) } } + listOfNotNull(me, home)
        val realmPts = if (pts.isEmpty()) realms.flatMap { r -> r.circle?.let { listOf(LatLng(it.center.lat, it.center.lon)) } ?: r.polygon.map { LatLng(it.lat, it.lon) } } else emptyList()
        val all = (pts + realmPts).distinctBy { it.latitude to it.longitude }
        if (all.isEmpty()) return@LaunchedEffect
        kotlinx.coroutines.delay(400)
        mapView.post {
            val bounds = org.maplibre.android.geometry.LatLngBounds.Builder().includes(all).build()
            // Points that are (nearly) the same spot would zoom in to the rooftops: keep a neighbourhood view instead.
            if (all.size == 1 || bounds.latitudeSpan < 0.004 && bounds.longitudeSpan < 0.004) m.moveCamera(CameraUpdateFactory.newLatLngZoom(bounds.center, 14.0))
            else m.moveCamera(CameraUpdateFactory.newLatLngBounds(bounds, 60))
        }
        centered = true
    }

    AndroidView(factory = { mapView }, modifier = modifier)
}

/** Move the camera (used by "center on me" / selecting a quest). */
fun centerOn(map: MapLibreMap?, p: LatLng, zoom: Double = 15.0) {
    map?.animateCamera(CameraUpdateFactory.newLatLngZoom(p, zoom))
}
