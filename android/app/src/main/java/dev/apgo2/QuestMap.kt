package dev.apgo2

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.PointF
import android.graphics.RectF
import android.view.MotionEvent
import android.view.View
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.apgo2.ui.METERS_PER_DEGREE
import dev.apgo2.ui.MapMarkers
import dev.apgo2.ui.circleRing
import kotlinx.coroutines.delay
import org.json.JSONObject
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdate
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.AttributionDialogManager
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapLibreMapOptions
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.Point
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.RealmOut
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot

private const val GRAB_DP = 32

// How far from a finger a pin, trail or park outline still counts as tapped.
private const val TAP_SLOP_DP = 16
private const val NOT_DRAGGING = -1
private const val RING_HANDLE = 1
private const val FRAME_PAD_DP = 24
private const val OVERVIEW_PAD_PX = 60
private const val SETTLE_MS = 400L
private const val RING_SETTLE_MS = 250L

/** How long the map takes to ease to new overlay padding; an overlay that slides should take as long, with [OverlayEasing]. */
internal const val OVERLAY_EASE_MS = 350

/** MapLibre's own camera easing curve, so a sliding overlay keeps pace with the map. */
internal val OverlayEasing = CubicBezierEasing(0.25f, 0.1f, 0.25f, 1f)
private const val FIT_ANIM_MS = 500
private const val FOCUS_MARGIN_DP = 16
private const val FOCUS_SCROLL_MS = 300L
private const val CALLOUT_PIN_DP = 20f
private const val NEIGHBOURHOOD_ZOOM = 14.0
private const val TIGHT_SPAN_DEG = 0.004
private const val MIN_FIT_POINTS = 2
private const val REVEAL_MS = 200
private const val CREDIT_BACKDROP_ALPHA = 0.75f

/** A find drawn on the map: an icon pin for its quest kind, coloured by the player's mark ("none" | "favorite" | "banned"). */
internal data class MapFind(
    val id: String,
    val at: LatLng,
    val kindId: String,
    val family: String,
    val mark: String,
    val selected: Boolean,
)

/** Ask the map to fit these points in view (inside the padding), after any padding change has settled. */
internal data class MapFit(
    val points: List<LatLng>,
    val nonce: Int,
)

/**
 * Ask the map to bring a point into view; [nonce] changes each time so the same point can be asked for twice. [roomAbovePx] is
 * the height of a callout that sits above the point. The map scrolls only as far as it takes to show the point and its callout in
 * the visible area, and not at all when they already show; it never zooms.
 */
internal data class MapFocus(
    val at: LatLng,
    val nonce: Int,
    val roomAbovePx: Int = 0,
)

// The newest values of what the caller passes in. The map's listeners live as long as the map, so they read these rather than the
// values of the composition they were created in.
private class LatestInputs(
    val onClick: State<(LatLng) -> Unit>,
    val onLongClick: State<((LatLng) -> Unit)?>,
    val handles: State<List<LatLng>>,
    val onFindClick: State<((String) -> Unit)?>,
    val onQuestClick: State<((Long, LatLng?) -> Unit)?>,
    val parkAt: State<((LatLng) -> Long?)?>,
    val anchor: State<LatLng?>,
    val onAnchor: State<((Offset?) -> Unit)?>,
    val overlayTopDp: State<Int>,
    val overlayBottomDp: State<Int>,
    val onHandleMove: State<((Int, LatLng) -> Unit)?>,
    val onHandleRelease: State<(() -> Unit)?>,
    val circle: State<Pair<LatLng, Double>?>,
)

// Picks up a handle (or a circle's ring) with the first touch and drags it; any other touch falls through to the map (pan, zoom, tap).
private class HandleDragger(
    private val inputs: LatestInputs,
    private val density: Float,
) {
    // Index of the handle being dragged, or NOT_DRAGGING.
    var dragging = NOT_DRAGGING
        private set

    fun onTouch(
        map: MapLibreMap,
        ev: MotionEvent,
    ): Boolean {
        val move = inputs.onHandleMove.value
        return when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                start(map, ev, canMove = move != null)
            }

            MotionEvent.ACTION_MOVE -> {
                (dragging >= 0).also { if (it) move?.invoke(dragging, map.projection.fromScreenLocation(PointF(ev.x, ev.y))) }
            }

            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                (dragging >= 0).also { wasDragging ->
                    dragging = NOT_DRAGGING
                    if (wasDragging) inputs.onHandleRelease.value?.invoke()
                }
            }

            else -> {
                dragging >= 0
            }
        }
    }

    private fun start(
        map: MapLibreMap,
        ev: MotionEvent,
        canMove: Boolean,
    ): Boolean {
        val grab = GRAB_DP * density

        fun px(p: LatLng) = map.projection.toScreenLocation(p)

        fun dist(a: PointF) = hypot((a.x - ev.x).toDouble(), (a.y - ev.y).toDouble())
        // A handle point wins; otherwise a touch on a circle's ring (invisible handle, index 1) resizes it.
        val hit =
            inputs.handles.value
                .withIndex()
                .minByOrNull { (_, h) -> dist(px(h)) }
                ?.takeIf { (_, h) -> dist(px(h)) < grab }
                ?.index
        val onRing = inputs.circle.value?.let { (c, r) -> abs(dist(px(c)) - ringRadiusPx(map, c, r)) < grab } == true
        dragging = if (canMove) hit ?: if (onRing) RING_HANDLE else NOT_DRAGGING else NOT_DRAGGING
        return dragging >= 0
    }

    // The circle's radius on screen, from its centre to the ring due east of it.
    private fun ringRadiusPx(
        map: MapLibreMap,
        c: LatLng,
        radiusM: Double,
    ): Double {
        val centre = map.projection.toScreenLocation(c)
        val edgeLon = c.longitude + radiusM / (METERS_PER_DEGREE * cos(Math.toRadians(c.latitude)))
        val edge = map.projection.toScreenLocation(LatLng(c.latitude, edgeLon))
        return hypot((edge.x - centre.x).toDouble(), (edge.y - centre.y).toDouble())
    }
}

// The map view with what hangs off it: the loaded style, the camera helpers and the touch handling. The camera starts at [start]
// (near what it will frame) so the first frame is never the whole world.
@Stable
private class MapHolder(
    context: Context,
    private val density: Float,
    private val inputs: LatestInputs,
    start: LatLng?,
) {
    val view: MapView
    var style by mutableStateOf<Style?>(null)
    var map by mutableStateOf<MapLibreMap?>(null)
    var centered by mutableStateOf(false)
    val dragger = HandleDragger(inputs, density)
    private val addedImages = mutableSetOf<String>()
    private var padApplied = false

    init {
        MapLibre.getInstance(context)
        // No MapLibre logo or "i" button: the map shows its data credit itself (see MapCredit).
        val options = MapLibreMapOptions.createFromAttributes(context).logoEnabled(false).attributionEnabled(false)
        start?.let {
            options.camera(
                CameraPosition
                    .Builder()
                    .target(it)
                    .zoom(NEIGHBOURHOOD_ZOOM)
                    .build(),
            )
        }
        view = MapView(context, options)
    }

    // Connect to the map once it exists: taps, drags, camera reports and the style.
    // The touch listener only feeds the handle dragger; the map is not a button, so it has no click to announce to accessibility.
    @SuppressLint("ClickableViewAccessibility")
    fun attach(m: MapLibreMap) {
        map = m
        m.addOnMapClickListener { ll -> onTap(m, ll) }
        view.setOnTouchListener { _, ev -> dragger.onTouch(m, ev) }
        m.addOnCameraMoveListener { reportAnchor(m) }
        m.addOnCameraIdleListener { reportAnchor(m) }
        m.addOnMapLongClickListener { ll -> inputs.onLongClick.value?.invoke(ll) != null }
        // Cluster rings appear as the camera moves, so their images are drawn when the map first asks for them.
        view.addOnStyleImageMissingListener { id -> m.style?.let { ensureImage(it, id) } }
        m.setStyle(Style.Builder().fromUri(MapStyle.URL)) { s ->
            MapStyle.install(s)
            style = s
        }
    }

    // Give the map a pin image the first time it is needed.
    fun ensureImage(
        s: Style,
        name: String,
    ) {
        if (!addedImages.add(name)) return
        MapMarkers.parse(name)?.let { s.addImage(name, MapMarkers.render(it)) }
    }

    // Replace the contents of a source.
    fun show(
        source: String,
        features: List<JSONObject>,
    ) {
        style?.getSourceAs<GeoJsonSource>(source)?.setGeoJson(GeoJson.collection(features))
    }

    // Draw again: a data change alone does not always redraw when the camera is still.
    fun repaint() = map?.triggerRepaint()

    // The map's padding is the part of it covered by overlays. Changing it keeps the camera target, so the point that was at the
    // centre of the visible map slides to the centre of the new visible area. The first value is applied at once, later ones ease,
    // unless the overlay moves smoothly by itself ([follow]): then each step is applied at once, so the map moves with it.
    fun applyPadding(
        topDp: Int,
        bottomDp: Int,
        follow: Boolean,
    ) {
        val m = map ?: return
        if (topDp == 0 && bottomDp == 0 && !padApplied) return
        val top = (topDp * density).toDouble()
        val bottom = (bottomDp * density).toDouble()
        if (padApplied && !follow) {
            m.easeCamera(CameraUpdateFactory.paddingTo(0.0, top, 0.0, bottom), OVERLAY_EASE_MS)
        } else {
            m.moveCamera(CameraUpdateFactory.paddingTo(0.0, top, 0.0, bottom))
            padApplied = true
        }
    }

    // The full list of the map's sources, with their links (MapLibre's own dialog, as its "i" button would open).
    fun showSources() {
        map?.let { AttributionDialogManager(view.context, it).onClick(view) }
    }

    // Fit the points in view, after a padding change has settled.
    suspend fun fit(f: MapFit) {
        val m = map ?: return
        if (f.points.size < MIN_FIT_POINTS) return
        delay(SETTLE_MS)
        m.animateCamera(fitTo(LatLngBounds.Builder().includes(f.points).build(), (FRAME_PAD_DP * density).toInt()), FIT_ANIM_MS)
    }

    // Tell the caller where its anchor point is on screen (null when it has none).
    fun reportAnchor(m: MapLibreMap) {
        inputs.onAnchor.value?.invoke(
            inputs.anchor.value?.let { a -> m.projection.toScreenLocation(a).let { Offset(it.x, it.y) } },
        )
    }

    // The visible area is the map minus the overlays. The point plus its callout form one block; centre that block in it. The camera
    // target sits at the centre of the padded view, so the top padding is chosen to put the target (the point) where it belongs.
    fun focusOn(f: MapFocus) {
        val m = map ?: return
        if (view.height <= 0) return
        // A refocus (the callout got measured) can come while the last one still moves: stop it and measure from where it is, so
        // the two do not add up.
        m.cancelTransitions()
        val p = m.projection.toScreenLocation(f.at)
        val visible =
            FocusShift.View(
                width = view.width.toFloat(),
                height = view.height.toFloat(),
                top = inputs.overlayTopDp.value * density,
                bottom = inputs.overlayBottomDp.value * density,
                margin = FOCUS_MARGIN_DP * density,
            )
        val above = f.roomAbovePx.toFloat()
        val below = CALLOUT_PIN_DP * density
        val shift = FocusShift.needed(p.x, p.y, above, below, visible)
        when {
            // Far away: centre it with its callout in the visible area, at the same zoom.
            FocusShift.far(shift, visible) -> nudge(m, FocusShift.centred(p.x, p.y, above, below, visible))

            shift != 0f to 0f -> nudge(m, shift)
        }
    }

    // Move the camera's centre by [shift] screen pixels. animateCamera, unlike scrollBy, reports the move, which keeps a callout on
    // its point.
    private fun nudge(
        m: MapLibreMap,
        shift: Pair<Float, Float>,
    ) {
        val c = m.projection.toScreenLocation(m.cameraPosition.target ?: return)
        val to = m.projection.fromScreenLocation(PointF(c.x + shift.first, c.y + shift.second))
        m.animateCamera(CameraUpdateFactory.newLatLng(to), FOCUS_SCROLL_MS.toInt())
    }

    // Keep a circle being edited fully in view as its radius changes (not when it only moves).
    suspend fun frameCircle(circle: Pair<LatLng, Double>) {
        val m = map ?: return
        val (c, r) = circle
        delay(RING_SETTLE_MS)
        if (dragger.dragging >= 0) return // never fight the finger
        val bounds = LatLngBounds.Builder().includes(circleRing(c.latitude, c.longitude, r).map { LatLng(it.first, it.second) }).build()
        view.post { m.animateCamera(fitTo(bounds, (FRAME_PAD_DP * density).toInt())) }
        centered = true
    }

    // Frame the action once: all visible quests plus you, or just you. Done after layout so the camera move is not dropped.
    suspend fun frameAction(points: List<LatLng>) {
        val m = map
        if (m == null || !canFrame(points)) return
        delay(SETTLE_MS)
        if (centered) return // a circle being edited already framed the map
        view.post { moveOverview(m, points) }
        centered = true
    }

    private fun canFrame(points: List<LatLng>) = style != null && !centered && points.isNotEmpty()

    // One spot, or points that are nearly the same, would zoom in to the rooftops: keep a neighbourhood view instead.
    private fun moveOverview(
        m: MapLibreMap,
        points: List<LatLng>,
    ) {
        val bounds = if (points.size >= MIN_FIT_POINTS) LatLngBounds.Builder().includes(points).build() else null
        val tight = bounds == null || (bounds.latitudeSpan < TIGHT_SPAN_DEG && bounds.longitudeSpan < TIGHT_SPAN_DEG)
        if (bounds == null || tight) {
            m.moveCamera(CameraUpdateFactory.newLatLngZoom(bounds?.center ?: points[0], NEIGHBOURHOOD_ZOOM))
        } else {
            m.moveCamera(fitTo(bounds, OVERVIEW_PAD_PX))
        }
    }

    // A bounds update replaces the map's padding with the padding it is given, so it must include the overlays to centre in the
    // visible area.
    private fun fitTo(
        bounds: LatLngBounds,
        pad: Int,
    ): CameraUpdate =
        CameraUpdateFactory.newLatLngBounds(
            bounds,
            pad,
            pad + (inputs.overlayTopDp.value * density).toInt(),
            pad,
            pad + (inputs.overlayBottomDp.value * density).toInt(),
        )

    // A tap on a cluster zooms in until it splits; on a find or quest pin (anywhere on it, head included) it selects it; any other
    // tap goes to the screen (e.g. adding a polygon corner).
    private fun onTap(
        m: MapLibreMap,
        ll: LatLng,
    ): Boolean {
        val at = m.projection.toScreenLocation(ll)
        if (zoomIntoCluster(m, at)) return true
        val onFind = inputs.onFindClick.value
        val onQuest = inputs.onQuestClick.value
        val slop = TAP_SLOP_DP * density
        val near = RectF(at.x - slop, at.y - slop, at.x + slop, at.y + slop)
        val find = onFind?.let { featureId(m, near, MapStyle.FIND_LAYERS) }
        // A pin wins over a trail, a trail over the park it may cross; then a tap anywhere inside a park picks it (asked of the
        // caller: an open park's fill is not drawn, so the map cannot find it).
        val pin = onQuest?.let { featureId(m, near, MapStyle.QUEST_LAYERS)?.toLongOrNull() }
        val shape =
            onQuest?.takeIf { pin == null }?.let {
                featureId(m, near, MapStyle.QUEST_LINE_LAYERS)?.toLongOrNull() ?: inputs.parkAt.value?.invoke(ll)
            }
        when {
            find != null -> onFind(find)

            pin != null -> onQuest(pin, null)

            shape != null -> onQuest(shape, ll)

            // a trail or park: its details show where it was touched
            else -> inputs.onClick.value(ll)
        }
        return true
    }

    private fun featureId(
        m: MapLibreMap,
        area: RectF,
        layers: Array<String>,
    ): String? = m.queryRenderedFeatures(area, *layers).firstOrNull()?.getStringProperty(MapProp.ID)

    private fun zoomIntoCluster(
        m: MapLibreMap,
        at: PointF,
    ): Boolean {
        val target =
            MapStyle.CLUSTER_LAYERS.firstNotNullOfOrNull { (layer, source) ->
                m.queryRenderedFeatures(at, layer).firstOrNull()?.let { expandCamera(m, it, source) }
            }
        target?.let { m.animateCamera(it, FIT_ANIM_MS) }
        return target != null
    }

    // Centre on the cluster at the zoom where it splits.
    private fun expandCamera(
        m: MapLibreMap,
        cluster: Feature,
        source: String,
    ): CameraUpdate? {
        val point = cluster.geometry() as? Point
        val zoom = m.style?.getSourceAs<GeoJsonSource>(source)?.getClusterExpansionZoom(cluster)
        return if (point == null ||
            zoom == null
        ) {
            null
        } else {
            CameraUpdateFactory.newLatLngZoom(LatLng(point.latitude(), point.longitude()), zoom.toDouble())
        }
    }
}

@Composable
internal fun QuestMap(
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
    /** True when the overlays slide smoothly by themselves (a dragged panel): the map follows each step instead of easing. */
    overlaysFollowed: Boolean = false,
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
    /** Tapping a quest pin calls this with its location id (a tap elsewhere goes to [onMapClick]). */
    onQuestClick: ((Long, LatLng?) -> Unit)? = null,
    /** The park quest a point is inside, if any: a tap there picks it like a tap on its outline. */
    parkAt: ((LatLng) -> Long?)? = null,
    /** Fly the camera here (kept clear of the bottom overlay). */
    focus: MapFocus? = null,
    fit: MapFit? = null,
    /**
     * A point to keep a callout attached to: [onAnchor] gets its screen position in the map's pixels (null when off screen) as
     * the camera moves.
     */
    anchor: LatLng? = null,
    onAnchor: ((Offset?) -> Unit)? = null,
    /** When false the drawn shape is only an outline: no handles, radius line, label or corner dots (and [onHandleMove] is not called). */
    editable: Boolean = true,
    /** Where you have been: one line per unbroken stretch of GPS. */
    trace: List<List<LatLng>> = emptyList(),
    /** Where to start when there is nothing to frame yet (the last place you were). */
    lastPlace: LatLng? = null,
    /** False while the map is kept but hidden (another tab is up): it stops drawing and keeps its camera, style and tiles. */
    onShow: Boolean = true,
) {
    val context = LocalContext.current
    val density = LocalDensity.current.density
    val inputs =
        LatestInputs(
            onClick = rememberUpdatedState(onMapClick),
            onLongClick = rememberUpdatedState(onMapLongClick),
            handles = rememberUpdatedState(handles),
            onFindClick = rememberUpdatedState(onFindClick),
            onQuestClick = rememberUpdatedState(onQuestClick),
            parkAt = rememberUpdatedState(parkAt),
            anchor = rememberUpdatedState(anchor),
            onAnchor = rememberUpdatedState(onAnchor),
            overlayTopDp = rememberUpdatedState(overlayTopDp),
            overlayBottomDp = rememberUpdatedState(overlayBottomDp),
            onHandleMove = rememberUpdatedState(onHandleMove),
            onHandleRelease = rememberUpdatedState(onHandleRelease),
            circle = rememberUpdatedState(circle),
        )
    val points = framePoints(quests, realms, me, home, draft)
    val holder = remember { MapHolder(context, density, inputs, MapStart.center(listOfNotNull(circle?.first) + points, lastPlace)) }
    MapLifecycle(holder.view, onShow)
    LaunchedEffect(holder) { holder.view.getMapAsync(holder::attach) }
    SyncContent(holder, quests, realms, selected, finds, trace)
    SyncDrawing(holder, draft, circle, editable, handles, handlesVisible)
    SyncPins(holder, thaw, waypoint, home, me)
    MapCamera(holder, overlayTopDp, overlayBottomDp, overlaysFollowed, fit, focus, anchor)
    MapFraming(holder, circle, me, quests, realms, points)
    // Covered until the first framing (or, with nothing to frame, until the style is in), then faded in: the camera jump and the
    // tiles loading happen out of sight.
    val framed = holder.centered || (holder.style != null && circle == null && points.isEmpty())
    val cover by animateFloatAsState(if (framed) 0f else 1f, tween(REVEAL_MS), label = "map cover")
    Box(modifier) {
        AndroidView(factory = { holder.view }, modifier = Modifier.matchParentSize())
        MapCredit(holder::showSources, Modifier.align(Alignment.TopEnd).padding(top = overlayTopDp.dp))
        if (cover > 0f) Box(Modifier.matchParentSize().alpha(cover).background(MaterialTheme.colorScheme.surface))
    }
}

// The map data's credit in the map's top corner (clear of a bottom panel and its grip), below whatever covers its top; a tap
// lists the sources.
@Composable
private fun MapCredit(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Text(
        MapStyle.CREDIT,
        modifier
            .padding(4.dp)
            .background(MaterialTheme.colorScheme.surface.copy(alpha = CREDIT_BACKDROP_ALPHA), RoundedCornerShape(4.dp))
            .clickable(onClickLabel = "Map data sources", onClick = onClick)
            .padding(horizontal = 4.dp, vertical = 1.dp),
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface,
    )
}

// Hands the map view the activity's lifecycle events while it is on show; a hidden map is stopped and made invisible (so it stops
// drawing), and a map leaving the screen is destroyed.
@Composable
private fun MapLifecycle(
    mapView: MapView,
    onShow: Boolean,
) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val life =
        remember(mapView) {
            MapLife({ mapView.onCreate(null) }, mapView::onStart, mapView::onResume, mapView::onPause, mapView::onStop, mapView::onDestroy)
        }
    val shown by rememberUpdatedState(onShow)
    DisposableEffect(lifecycle, life) {
        val observer = LifecycleEventObserver { _, _ -> life.update(lifecycle.currentState, shown) }
        lifecycle.addObserver(observer)
        onDispose {
            lifecycle.removeObserver(observer)
            life.destroy()
        }
    }
    LaunchedEffect(life, onShow) {
        mapView.visibility = if (onShow) View.VISIBLE else View.INVISIBLE
        life.update(lifecycle.currentState, onShow)
    }
}

// The realms, finds, quests and your trace.
@Composable
private fun SyncContent(
    holder: MapHolder,
    quests: List<QuestOut>,
    realms: List<RealmOut>,
    selected: Long?,
    finds: List<MapFind>,
    trace: List<List<LatLng>>,
) {
    val style = holder.style
    LaunchedEffect(style, realms) { holder.show(MapSource.REALMS, MapFeatures.realms(realms)) }
    LaunchedEffect(style, finds) {
        style?.let { st -> finds.map { it.mapImageKey }.toSet().forEach { holder.ensureImage(st, it) } }
        val (rest, picked) = MapFeatures.splitSelected(MapFeatures.finds(finds))
        holder.show(MapSource.FINDS, rest)
        holder.show(MapSource.FIND_SEL, picked)
    }
    LaunchedEffect(style, quests, selected) {
        style?.let { st -> quests.forEach { holder.ensureImage(st, it.mapImageKey) } }
        val (rest, picked) = MapFeatures.splitSelected(MapFeatures.quests(quests, selected))
        holder.show(MapSource.QUESTS, rest)
        holder.show(MapSource.QUEST_SEL, picked)
        holder.show(MapSource.LINES, MapFeatures.lines(quests))
        holder.show(MapSource.AREAS, MapFeatures.areas(quests))
    }
    LaunchedEffect(style, trace) { holder.show(MapSource.TRACE, MapFeatures.trace(trace)) }
}

// The shape being drawn, the circle's radius and the handles.
@Composable
private fun SyncDrawing(
    holder: MapHolder,
    draft: List<LatLng>,
    circle: Pair<LatLng, Double>?,
    editable: Boolean,
    handles: List<LatLng>,
    handlesVisible: Boolean,
) {
    val style = holder.style
    LaunchedEffect(style, draft, circle, editable) { holder.show(MapSource.DRAFT, MapFeatures.draft(draft, circle, editable)) }
    LaunchedEffect(style, circle, editable) {
        val radius = MapFeatures.radius(circle, editable)
        holder.show(MapSource.RADIUS, radius?.line.orEmpty())
        holder.show(MapSource.RING_KNOBS, radius?.knobs.orEmpty())
        holder.show(MapSource.RING_LABEL, radius?.label.orEmpty())
    }
    LaunchedEffect(style, handles, handlesVisible) {
        holder.show(MapSource.HANDLES, if (handlesVisible) handles.flatMap { MapFeatures.pin(it) } else emptyList())
        holder.repaint()
    }
}

// The thaw point, the detour waypoint, home and you.
@Composable
private fun SyncPins(
    holder: MapHolder,
    thaw: LatLng?,
    waypoint: LatLng?,
    home: LatLng?,
    me: LatLng?,
) {
    val style = holder.style
    LaunchedEffect(style, thaw, waypoint) { holder.show(MapSource.MARKS, MapFeatures.marks(thaw, waypoint)) }
    LaunchedEffect(style, home) {
        holder.show(MapSource.HOME, MapFeatures.pin(home))
        holder.repaint()
    }
    LaunchedEffect(style, me) { holder.show(MapSource.ME, MapFeatures.pin(me)) }
}

// Moves the camera when the caller asks: overlay padding, fit these points, focus this point, keep the anchor reported.
@Composable
private fun MapCamera(
    holder: MapHolder,
    overlayTopDp: Int,
    overlayBottomDp: Int,
    overlaysFollowed: Boolean,
    fit: MapFit?,
    focus: MapFocus?,
    anchor: LatLng?,
) {
    val map = holder.map
    LaunchedEffect(map, overlayTopDp, overlayBottomDp) { holder.applyPadding(overlayTopDp, overlayBottomDp, overlaysFollowed) }
    LaunchedEffect(fit) { fit?.let { holder.fit(it) } }
    LaunchedEffect(anchor, map) { map?.let { holder.reportAnchor(it) } }
    LaunchedEffect(focus) { focus?.let { holder.focusOn(it) } }
}

// The first framing of the map: around a circle being edited, otherwise around the action.
@Composable
private fun MapFraming(
    holder: MapHolder,
    circle: Pair<LatLng, Double>?,
    me: LatLng?,
    quests: List<QuestOut>,
    realms: List<RealmOut>,
    points: List<LatLng>,
) {
    val style = holder.style
    LaunchedEffect(style, circle?.second) { if (style != null && circle != null) holder.frameCircle(circle) }
    LaunchedEffect(style, me, quests.isNotEmpty(), realms.size) { holder.frameAction(points) }
}

// What the first view should show: the visible quests, you, home and the shape being drawn; the realms when there is none of those.
private fun framePoints(
    quests: List<QuestOut>,
    realms: List<RealmOut>,
    me: LatLng?,
    home: LatLng?,
    draft: List<LatLng>,
): List<LatLng> {
    val pts =
        quests.filter { it.state != "hidden" }.mapNotNull { q -> q.anchor?.let { LatLng(it.lat, it.lon) } } + listOfNotNull(me, home) +
            draft
    val realmPts = if (pts.isEmpty()) realms.flatMap(::realmPoints) else emptyList()
    return (pts + realmPts).distinctBy { it.latitude to it.longitude }
}

private fun realmPoints(r: RealmOut): List<LatLng> =
    if (r.polygonActive) {
        r.polygon.map { LatLng(it.lat, it.lon) }
    } else {
        listOfNotNull(r.circle?.let { LatLng(it.center.lat, it.center.lon) })
    }
