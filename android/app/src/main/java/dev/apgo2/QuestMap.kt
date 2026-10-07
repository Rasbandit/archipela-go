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
import android.graphics.PointF
import android.view.MotionEvent
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin
import org.json.JSONArray
import org.json.JSONObject
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.layers.PropertyFactory.circleColor
import org.maplibre.android.style.layers.PropertyFactory.circleRadius
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeColor
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeWidth
import org.maplibre.android.style.layers.PropertyFactory.fillColor
import org.maplibre.android.style.layers.PropertyFactory.fillOpacity
import org.maplibre.android.style.layers.PropertyFactory.iconAllowOverlap
import org.maplibre.android.style.layers.PropertyFactory.iconIgnorePlacement
import org.maplibre.android.style.layers.PropertyFactory.iconImage
import org.maplibre.android.style.layers.PropertyFactory.iconOpacity
import org.maplibre.android.style.layers.PropertyFactory.iconSize
import org.maplibre.android.style.layers.PropertyFactory.lineColor
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory.symbolSortKey
import org.maplibre.android.style.layers.PropertyFactory.textAllowOverlap
import org.maplibre.android.style.layers.PropertyFactory.textAnchor
import org.maplibre.android.style.layers.PropertyFactory.textOffset
import org.maplibre.android.style.layers.PropertyFactory.textColor
import org.maplibre.android.style.layers.PropertyFactory.textField
import org.maplibre.android.style.layers.PropertyFactory.textFont
import org.maplibre.android.style.layers.PropertyFactory.textHaloColor
import org.maplibre.android.style.layers.PropertyFactory.textHaloWidth
import org.maplibre.android.style.layers.PropertyFactory.textIgnorePlacement
import org.maplibre.android.style.layers.PropertyFactory.textSize
import org.maplibre.android.style.layers.PropertyFactory.lineWidth
import org.maplibre.android.style.sources.GeoJsonSource
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.circleRing
import dev.apgo2.ui.renderGlyph
import dev.apgo2.ui.renderMarker
import dev.apgo2.ui.renderPin
import dev.apgo2.ui.hex
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.RealmOut

/** A find drawn on the map: an icon pin for its quest kind, coloured by the player's mark ("none" | "favorite" | "banned"). */
data class MapFind(val id: String, val at: LatLng, val kindId: String, val family: String, val mark: String, val selected: Boolean)

/** Ask the map to fit these points in view (inside the padding), after any padding change has settled. */
data class MapFit(val points: List<LatLng>, val nonce: Int)

/**
 * Ask the map to bring a point into view; [nonce] changes each time so the same point can be asked for twice. [roomAbovePx] is the height of a
 * callout that sits above the point: the point and its callout are centred together in the visible area. Zooms in only when the map is zoomed out.
 */
data class MapFocus(val at: LatLng, val nonce: Int, val roomAbovePx: Int = 0)

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

private fun glyphName(q: QuestOut) = "glyph|${q.kindId}|${q.family}"

private fun findImage(f: MapFind) = "pin|${f.kindId}|${f.family}|${f.mark}"

private fun questFeatures(quests: List<QuestOut>, selected: Long?): List<JSONObject> =
    quests.filter { it.state != "hidden" && it.anchor != null && it.shape != "line" }.map {
        val a = it.anchor!!
        feature(
            pointGeo(a.lat, a.lon),
            JSONObject().put("state", it.state).put("diff", if (it.boss) "boss" else it.difficulty).put("sel", it.locationId == selected).put("img", glyphName(it)),
        )
    } + quests.filter { it.state != "hidden" && it.shape == "courier" && it.anchorB != null }.map {
        val b = it.anchorB!!
        feature(pointGeo(b.lat, b.lon), JSONObject().put("state", it.state).put("diff", "easy").put("sel", it.locationId == selected).put("img", glyphName(it)))
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
    val circlePts = r.circle?.let { circleRing(it.center.lat, it.center.lon, it.radiusM) }
    val pts = if (r.polygonActive) r.polygon.takeIf { it.size >= 3 }?.map { it.lat to it.lon } else circlePts
    pts?.let {
        feature(JSONObject().put("type", "Polygon").put("coordinates", JSONArray().put(ring(it))), JSONObject().put("name", r.name))
    }
}

private fun draftFeatures(draft: List<LatLng>, circle: Pair<LatLng, Double>?, editable: Boolean): List<JSONObject> {
    val out = mutableListOf<JSONObject>()
    if (circle != null) {
        val (c, r) = circle
        out += feature(JSONObject().put("type", "Polygon").put("coordinates", JSONArray().put(ring(circleRing(c.latitude, c.longitude, r)))))
        if (editable) out += feature(pointGeo(c.latitude, c.longitude)) // the centre handle's dot
    }
    if (draft.size >= 3) out += feature(JSONObject().put("type", "Polygon").put("coordinates", JSONArray().put(ring(draft.map { it.latitude to it.longitude }))))
    else if (draft.size == 2) out += feature(JSONObject().put("type", "LineString").put("coordinates", JSONArray().put(coord(draft[0].latitude, draft[0].longitude)).put(coord(draft[1].latitude, draft[1].longitude))))
    if (editable) draft.forEach { out += feature(pointGeo(it.latitude, it.longitude)) } // the corner dots
    return out
}

private fun stateColor() = Expression.match(
    Expression.get("state"),
    Expression.literal(ApgoPalette.questTodo.hex()),
    Expression.stop("locked", Expression.literal(ApgoPalette.questLocked.hex())),
    Expression.stop("done", Expression.literal(ApgoPalette.questDone.hex())),
    Expression.stop("progress", Expression.literal(ApgoPalette.questProgress.hex())),
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
    /** Circle being edited: center and radius in metres. */
    circle: Pair<LatLng, Double>? = null,
    /** Height of overlays covering the top and bottom of the map, so framing keeps the circle clear of them. */
    overlayTopDp: Int = 0,
    overlayBottomDp: Int = 0,
    /** Points the user can pick up and drag; [onHandleMove] gets the handle index and its new position. */
    handles: List<LatLng> = emptyList(),
    onHandleMove: ((Int, LatLng) -> Unit)? = null,
    /** False hides the handles' own drawing (the thing being dragged draws itself) while they can still be grabbed. */
    handlesVisible: Boolean = true,
    /** Called when a handle drag ends (the finger lifts). */
    onHandleRelease: (() -> Unit)? = null,
    /** Finds drawn as icon pins; tapping one calls [onFindClick] with its id. */
    finds: List<MapFind> = emptyList(),
    onFindClick: ((String) -> Unit)? = null,
    /** Fly the camera here (kept clear of the bottom overlay). */
    focus: MapFocus? = null,
    fit: MapFit? = null,
    /** A point to keep a callout attached to: [onAnchor] gets its screen position in the map's pixels (null when off screen) as the camera moves. */
    anchor: LatLng? = null,
    onAnchor: ((androidx.compose.ui.geometry.Offset?) -> Unit)? = null,
    /** When false the drawn shape is only an outline: no handles, radius line, label or corner dots (and [onHandleMove] is not called). */
    editable: Boolean = true,
) {
    val context = LocalContext.current
    val density = androidx.compose.ui.platform.LocalDensity.current.density
    val mapView = remember {
        MapLibre.getInstance(context)
        MapView(context)
    }
    var style by remember { mutableStateOf<Style?>(null) }
    var map by remember { mutableStateOf<MapLibreMap?>(null) }
    var centered by remember { mutableStateOf(false) }
    val clickHandler by rememberUpdatedState(onMapClick)
    val longClickHandler by rememberUpdatedState(onMapLongClick)
    val handlesNow by rememberUpdatedState(handles)
    val findClickNow by rememberUpdatedState(onFindClick)
    val addedImages = remember { mutableSetOf<String>() }
    val padApplied = remember { booleanArrayOf(false) }
    val anchorNow by rememberUpdatedState(anchor)
    val onAnchorNow by rememberUpdatedState(onAnchor)
    val overlayTopNow by rememberUpdatedState(overlayTopDp)
    val overlayBottomNow by rememberUpdatedState(overlayBottomDp)
    // A bounds update replaces the map's padding with the padding it is given, so it must include the overlays to centre in the visible area.
    fun fitTo(bounds: org.maplibre.android.geometry.LatLngBounds, pad: Int) =
        CameraUpdateFactory.newLatLngBounds(bounds, pad, pad + (overlayTopNow * density).toInt(), pad, pad + (overlayBottomNow * density).toInt())
    val moveNow by rememberUpdatedState(onHandleMove)
    val releaseNow by rememberUpdatedState(onHandleRelease)
    val circleNow by rememberUpdatedState(circle)
    val dragging = remember { intArrayOf(-1) } // index of the handle being dragged, or -1

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
            m.addOnMapClickListener { ll ->
                // A tap on a find pin selects it; any other tap goes to the screen (e.g. adding a polygon corner).
                val hit = findClickNow?.let { _ -> m.queryRenderedFeatures(m.projection.toScreenLocation(ll), "finds-layer", "finds-sel").firstOrNull() }
                if (hit != null) findClickNow?.invoke(hit.getStringProperty("id")) else clickHandler(ll)
                true
            }
            // A touch that starts on a handle drags it; anything else falls through to the map (pan, zoom, tap).
            mapView.setOnTouchListener { _, ev ->
                val move = moveNow
                when (ev.actionMasked) {
                    MotionEvent.ACTION_DOWN -> {
                        fun px(p: LatLng) = m.projection.toScreenLocation(p).let { PointF(it.x, it.y) }
                        fun dist(a: PointF) = hypot((a.x - ev.x).toDouble(), (a.y - ev.y).toDouble())
                        val grab = 32 * density
                        // A handle point wins; otherwise a touch on a circle's ring (invisible handle, index 1) resizes it.
                        val hit = handlesNow.withIndex().minByOrNull { (_, h) -> dist(px(h)) }?.takeIf { (_, h) -> dist(px(h)) < grab }?.index
                        val onRing = circleNow?.let { (c, r) ->
                            val centre = px(c)
                            val edge = px(LatLng(c.latitude, c.longitude + r / (111_195.0 * cos(Math.toRadians(c.latitude)))))
                            abs(hypot((centre.x - ev.x).toDouble(), (centre.y - ev.y).toDouble()) - hypot((edge.x - centre.x).toDouble(), (edge.y - centre.y).toDouble())) < grab
                        } == true
                        dragging[0] = if (move == null) -1 else hit ?: if (onRing) 1 else -1
                        dragging[0] >= 0
                    }
                    MotionEvent.ACTION_MOVE -> (dragging[0] >= 0).also { if (it) move?.invoke(dragging[0], m.projection.fromScreenLocation(PointF(ev.x, ev.y))) }
                    MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> (dragging[0] >= 0).also { wasDragging -> dragging[0] = -1; if (wasDragging) releaseNow?.invoke() }
                    else -> dragging[0] >= 0
                }
            }
            fun reportAnchor() = onAnchorNow?.invoke(anchorNow?.let { a -> m.projection.toScreenLocation(a).let { androidx.compose.ui.geometry.Offset(it.x, it.y) } })
            m.addOnCameraMoveListener(::reportAnchor)
            m.addOnCameraIdleListener(::reportAnchor)
            m.addOnMapLongClickListener { ll -> longClickHandler?.invoke(ll) != null }
            m.setStyle(Style.Builder().fromUri(STYLE_URL)) { s ->
                val empty = fc(emptyList())
                listOf("realms", "areas", "lines", "quests", "finds", "draft", "marks", "home", "handles", "radius", "ringknobs", "ringlabel", "me").forEach { s.addSource(GeoJsonSource(it, empty)) }
                s.addLayer(FillLayer("realms-fill", "realms").withProperties(fillColor(ApgoPalette.realm.hex()), fillOpacity(0.07f)))
                s.addLayer(LineLayer("realms-line", "realms").withProperties(lineColor(ApgoPalette.realm.hex()), lineWidth(1.8f)))
                s.addLayer(FillLayer("areas-fill", "areas").withProperties(fillColor(stateColor()), fillOpacity(0.18f)))
                s.addLayer(LineLayer("lines-layer", "lines").withProperties(lineColor(stateColor()), lineWidth(4f)))
                s.addLayer(
                    CircleLayer("quests-sel", "quests").withFilter(Expression.eq(Expression.get("sel"), Expression.literal(true))).withProperties(
                        circleRadius(21f), circleColor(ApgoPalette.onMap.hex()), circleStrokeColor(ApgoPalette.realm.hex()), circleStrokeWidth(3f),
                    ),
                )
                s.addLayer(
                    CircleLayer("quests-layer", "quests").withProperties(
                        circleRadius(
                            Expression.match(
                                Expression.get("diff"), Expression.literal(8f),
                                Expression.stop("easy", Expression.literal(9f)),
                                Expression.stop("medium", Expression.literal(11f)),
                                Expression.stop("hard", Expression.literal(13f)),
                                Expression.stop("boss", Expression.literal(16f)),
                            ),
                        ),
                        circleColor(stateColor()), circleStrokeColor(ApgoPalette.onMap.hex()), circleStrokeWidth(1.5f),
                    ),
                )
                s.addLayer(SymbolLayer("quests-icons", "quests").withProperties(iconImage(Expression.get("img")), iconSize(0.38f), iconAllowOverlap(true), iconIgnorePlacement(true)))
                // Finds: icon pins that thin out by collision, favorites winning over plain ones and banned ones; the selected find always shows.
                s.addLayer(
                    SymbolLayer("finds-layer", "finds").withProperties(
                        iconImage(Expression.get("img")), iconSize(0.62f), iconAllowOverlap(false), iconIgnorePlacement(false),
                        symbolSortKey(Expression.get("z")), iconOpacity(Expression.get("op")),
                    ),
                )
                s.addLayer(
                    SymbolLayer("finds-sel", "finds").withFilter(Expression.eq(Expression.get("sel"), Expression.literal(true))).withProperties(
                        iconImage(Expression.get("img")), iconSize(0.92f), iconAllowOverlap(true), iconIgnorePlacement(true),
                    ),
                )
                s.addLayer(LineLayer("draft-line", "draft").withProperties(lineColor(ApgoPalette.draft.hex()), lineWidth(3f)))
                s.addLayer(FillLayer("draft-fill", "draft").withProperties(fillColor(ApgoPalette.draft.hex()), fillOpacity(0.15f)))
                s.addLayer(CircleLayer("draft-pts", "draft").withFilter(Expression.eq(Expression.geometryType(), Expression.literal("Point"))).withProperties(circleRadius(5f), circleColor(ApgoPalette.draft.hex()), circleStrokeColor(ApgoPalette.onMap.hex()), circleStrokeWidth(1.5f)))
                s.addLayer(CircleLayer("marks-layer", "marks").withProperties(circleRadius(12f), circleColor(Expression.get("color")), circleStrokeColor(ApgoPalette.onMap.hex()), circleStrokeWidth(3f)))
                s.addLayer(CircleLayer("handles-layer", "handles").withProperties(circleRadius(11f), circleColor(ApgoPalette.onMap.hex()), circleStrokeColor(ApgoPalette.draft.hex()), circleStrokeWidth(3.5f)))
                // The circle's radius: a line from the centre to the ring with the value above it, and a grip knob on the ring (the whole ring is draggable).
                s.addLayer(LineLayer("radius-line", "radius").withProperties(lineColor(ApgoPalette.draftStrong.hex()), lineWidth(2.5f)))
                s.addLayer(
                    CircleLayer("ringknobs-layer", "ringknobs").withProperties(
                        circleRadius(6f), circleColor(ApgoPalette.onMap.hex()), circleStrokeColor(ApgoPalette.draft.hex()), circleStrokeWidth(3f),
                    ),
                )
                s.addLayer(
                    SymbolLayer("ringlabel-layer", "ringlabel").withProperties(
                        textField(Expression.get("label")), textFont(arrayOf("Noto Sans Bold")), textSize(14f),
                        textColor(ApgoPalette.draftStrong.hex()), textHaloColor(ApgoPalette.onMap.hex()), textHaloWidth(2.5f),
                        textAllowOverlap(true), textIgnorePlacement(true),
                        textAnchor(Property.TEXT_ANCHOR_BOTTOM), textOffset(arrayOf(0f, -0.3f)),
                    ),
                )
                // You and home are badges: a person on blue, a house on green.
                s.addImage("badge-me", renderPin(ApgoIcons.Me, 120, fill = ApgoPalette.me))
                s.addImage("marker-home", renderMarker(ApgoIcons.Home, 168, ApgoPalette.home))
                s.addLayer(SymbolLayer("home-layer", "home").withProperties(iconImage("marker-home"), iconSize(0.8f), iconAllowOverlap(true), iconIgnorePlacement(true)))
                s.addLayer(SymbolLayer("me-layer", "me").withProperties(iconImage("badge-me"), iconSize(0.75f), iconAllowOverlap(true), iconIgnorePlacement(true)))
                style = s
            }
        }
    }

    LaunchedEffect(style, realms) { style?.getSourceAs<GeoJsonSource>("realms")?.setGeoJson(fc(realmFeatures(realms))) }
    fun ensureImage(s: Style, name: String) {
        if (!addedImages.add(name)) return
        val parts = name.split("|")
        val icon = ApgoIcons.forKind(parts[1], parts[2])
        s.addImage(
            name,
            if (parts[0] == "pin") {
                when (parts[3]) {
                    "favorite" -> renderPin(icon, 96, fill = ApgoPalette.kind(parts[1], parts[2]), ring = ApgoPalette.favorite, ringFraction = 0.13f)
                    "banned" -> renderPin(icon, 96, fill = ApgoPalette.muted)
                    else -> renderPin(icon, 96, fill = ApgoPalette.kind(parts[1], parts[2]))
                }
            } else {
                renderGlyph(icon, 48)
            },
        )
    }
    LaunchedEffect(style, finds) {
        val st = style ?: return@LaunchedEffect
        finds.map(::findImage).toSet().forEach { ensureImage(st, it) }
        st.getSourceAs<GeoJsonSource>("finds")?.setGeoJson(
            fc(
                finds.map {
                    feature(
                        pointGeo(it.at.latitude, it.at.longitude),
                        JSONObject().put("id", it.id).put("img", findImage(it)).put("sel", it.selected)
                            .put("z", when (it.mark) { "favorite" -> 0; "banned" -> 2; else -> 1 }).put("op", if (it.mark == "banned") 0.55 else 1.0),
                    )
                },
            ),
        )
    }
    // The map's padding is the part of it covered by overlays. Changing it keeps the camera target, so the point that was at the centre of the
    // visible map slides to the centre of the new visible area. The first value is applied at once, later ones ease.
    LaunchedEffect(map, overlayTopDp, overlayBottomDp) {
        val m = map ?: return@LaunchedEffect
        if (overlayTopDp == 0 && overlayBottomDp == 0 && !padApplied[0]) return@LaunchedEffect
        val top = overlayTopDp * density
        val bottom = overlayBottomDp * density
        if (!padApplied[0]) { m.setPadding(0, top.toInt(), 0, bottom.toInt()); padApplied[0] = true }
        else m.easeCamera(CameraUpdateFactory.paddingTo(0.0, top.toDouble(), 0.0, bottom.toDouble()), 350)
    }
    LaunchedEffect(fit) {
        val f = fit ?: return@LaunchedEffect
        val m = map ?: return@LaunchedEffect
        if (f.points.size < 2) return@LaunchedEffect
        kotlinx.coroutines.delay(400) // let a padding change settle first
        val bounds = org.maplibre.android.geometry.LatLngBounds.Builder().includes(f.points).build()
        val pad = (24 * density).toInt()
        m.animateCamera(fitTo(bounds, pad), 500)
    }
    LaunchedEffect(anchor, map) {
        val m = map ?: return@LaunchedEffect
        onAnchorNow?.invoke(anchor?.let { a -> m.projection.toScreenLocation(a).let { androidx.compose.ui.geometry.Offset(it.x, it.y) } })
    }
    LaunchedEffect(focus) {
        val f = focus ?: return@LaunchedEffect
        val m = map ?: return@LaunchedEffect
        val height = mapView.height.toFloat()
        if (height <= 0f) return@LaunchedEffect
        // The visible area is the map minus the overlays. The point plus its callout form one block; centre that block in it.
        // The camera target sits at the centre of the padded view, so the top padding is chosen to put the target (the point) where it belongs.
        val bottom = overlayBottomNow * density
        val top0 = overlayTopNow * density
        val block = f.roomAbovePx + 20f * density // the callout, then the pin itself
        val pinY = top0 + (height - bottom - top0 - block).coerceAtLeast(0f) / 2f + f.roomAbovePx
        val top = (2f * pinY - height + bottom).coerceAtLeast(top0)
        val zoom = if (m.cameraPosition.zoom < 16.0) 17.0 else m.cameraPosition.zoom
        m.animateCamera(CameraUpdateFactory.newCameraPosition(CameraPosition.Builder().target(f.at).zoom(zoom).padding(0.0, top.toDouble(), 0.0, bottom.toDouble()).build()))
    }
    LaunchedEffect(style, quests, selected) {
        style?.let { st -> quests.forEach { ensureImage(st, glyphName(it)) } }
        style?.getSourceAs<GeoJsonSource>("quests")?.setGeoJson(fc(questFeatures(quests, selected)))
        style?.getSourceAs<GeoJsonSource>("lines")?.setGeoJson(fc(lineFeatures(quests)))
        style?.getSourceAs<GeoJsonSource>("areas")?.setGeoJson(fc(areaFeatures(quests)))
    }
    LaunchedEffect(style, draft, circle, editable) { style?.getSourceAs<GeoJsonSource>("draft")?.setGeoJson(fc(draftFeatures(draft, circle, editable))) }
    LaunchedEffect(style, thaw, waypoint) {
        val marks = mutableListOf<JSONObject>()
        thaw?.let { marks += feature(pointGeo(it.latitude, it.longitude), JSONObject().put("color", ApgoPalette.thaw.hex())) }
        waypoint?.let { marks += feature(pointGeo(it.latitude, it.longitude), JSONObject().put("color", ApgoPalette.waypoint.hex())) }
        style?.getSourceAs<GeoJsonSource>("marks")?.setGeoJson(fc(marks))
    }
    LaunchedEffect(style, circle, editable) {
        val geo = circle?.takeIf { editable }?.let { (c, r) ->
            val dLon = r / (111_195.0 * cos(Math.toRadians(c.latitude)))
            val text = if (r < 1000) "${r.toInt()} m" else "%.1f km".format(r / 1000)
            val line = JSONObject().put("type", "LineString").put("coordinates", JSONArray().put(coord(c.latitude, c.longitude)).put(coord(c.latitude, c.longitude + dLon)))
            val knobs = listOf(feature(pointGeo(c.latitude, c.longitude + dLon)))
            Triple(listOf(feature(line)), knobs, listOf(feature(pointGeo(c.latitude, c.longitude + dLon / 2), JSONObject().put("label", text))))
        }
        style?.getSourceAs<GeoJsonSource>("radius")?.setGeoJson(fc(geo?.first ?: emptyList()))
        style?.getSourceAs<GeoJsonSource>("ringknobs")?.setGeoJson(fc(geo?.second ?: emptyList()))
        style?.getSourceAs<GeoJsonSource>("ringlabel")?.setGeoJson(fc(geo?.third ?: emptyList()))
    }
    LaunchedEffect(style, handles, handlesVisible) {
        style?.getSourceAs<GeoJsonSource>("handles")?.setGeoJson(fc(if (handlesVisible) handles.map { feature(pointGeo(it.latitude, it.longitude)) } else emptyList()))
        map?.triggerRepaint()
    }
    LaunchedEffect(style, home) {
        style?.getSourceAs<GeoJsonSource>("home")?.setGeoJson(fc(home?.let { listOf(feature(pointGeo(it.latitude, it.longitude))) } ?: emptyList()))
        map?.triggerRepaint() // a data change alone does not always redraw when the camera is still
    }
    LaunchedEffect(style, me) {
        style?.getSourceAs<GeoJsonSource>("me")?.setGeoJson(fc(me?.let { listOf(feature(pointGeo(it.latitude, it.longitude))) } ?: emptyList()))
    }
    // Keep a circle being edited fully in view as its radius changes (not when it only moves).
    LaunchedEffect(style, circle?.second) {
        val m = map ?: return@LaunchedEffect
        val (c, r) = circle ?: return@LaunchedEffect
        if (style == null) return@LaunchedEffect
        kotlinx.coroutines.delay(250)
        if (dragging[0] >= 0) return@LaunchedEffect // never fight the finger
        val b = org.maplibre.android.geometry.LatLngBounds.Builder().includes(circleRing(c.latitude, c.longitude, r).map { LatLng(it.first, it.second) }).build()
        val pad = (24 * density).toInt()
        mapView.post { m.animateCamera(fitTo(b, pad)) }
        centered = true
    }
    // Frame the action once: all visible quests plus you, or just you. Done after layout so the camera move is not dropped.
    LaunchedEffect(style, me, quests.isNotEmpty(), realms.size) {
        val m = map ?: return@LaunchedEffect
        if (style == null || centered) return@LaunchedEffect
        val pts = quests.filter { it.state != "hidden" }.mapNotNull { q -> q.anchor?.let { LatLng(it.lat, it.lon) } } + listOfNotNull(me, home) + draft
        val realmPts = if (pts.isEmpty()) realms.flatMap { r -> if (r.polygonActive) r.polygon.map { LatLng(it.lat, it.lon) } else listOfNotNull(r.circle?.let { LatLng(it.center.lat, it.center.lon) }) } else emptyList()
        val all = (pts + realmPts).distinctBy { it.latitude to it.longitude }
        if (all.isEmpty()) return@LaunchedEffect
        kotlinx.coroutines.delay(400)
        if (centered) return@LaunchedEffect // a circle being edited already framed the map
        mapView.post {
            val bounds = if (all.size >= 2) org.maplibre.android.geometry.LatLngBounds.Builder().includes(all).build() else null
            // One spot, or points that are nearly the same, would zoom in to the rooftops: keep a neighbourhood view instead.
            if (bounds == null || bounds.latitudeSpan < 0.004 && bounds.longitudeSpan < 0.004) m.moveCamera(CameraUpdateFactory.newLatLngZoom(bounds?.center ?: all[0], 14.0))
            else m.moveCamera(fitTo(bounds, 60))
        }
        centered = true
    }

    AndroidView(factory = { mapView }, modifier = modifier)
}

/** Move the camera (used by "center on me" / selecting a quest). */
fun centerOn(map: MapLibreMap?, p: LatLng, zoom: Double = 15.0) {
    map?.animateCamera(CameraUpdateFactory.newLatLngZoom(p, zoom))
}
