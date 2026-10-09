package dev.apgo2

import androidx.compose.ui.graphics.Color
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.MapMarkers
import dev.apgo2.ui.hex
import dev.apgo2.ui.renderMarker
import dev.apgo2.ui.renderPin
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.Layer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory.circleColor
import org.maplibre.android.style.layers.PropertyFactory.circleRadius
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeColor
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeWidth
import org.maplibre.android.style.layers.PropertyFactory.fillColor
import org.maplibre.android.style.layers.PropertyFactory.fillOpacity
import org.maplibre.android.style.layers.PropertyFactory.iconAllowOverlap
import org.maplibre.android.style.layers.PropertyFactory.iconAnchor
import org.maplibre.android.style.layers.PropertyFactory.iconIgnorePlacement
import org.maplibre.android.style.layers.PropertyFactory.iconImage
import org.maplibre.android.style.layers.PropertyFactory.iconOpacity
import org.maplibre.android.style.layers.PropertyFactory.iconSize
import org.maplibre.android.style.layers.PropertyFactory.lineCap
import org.maplibre.android.style.layers.PropertyFactory.lineColor
import org.maplibre.android.style.layers.PropertyFactory.lineDasharray
import org.maplibre.android.style.layers.PropertyFactory.lineJoin
import org.maplibre.android.style.layers.PropertyFactory.lineOpacity
import org.maplibre.android.style.layers.PropertyFactory.lineWidth
import org.maplibre.android.style.layers.PropertyFactory.symbolSortKey
import org.maplibre.android.style.layers.PropertyFactory.textAllowOverlap
import org.maplibre.android.style.layers.PropertyFactory.textAnchor
import org.maplibre.android.style.layers.PropertyFactory.textColor
import org.maplibre.android.style.layers.PropertyFactory.textField
import org.maplibre.android.style.layers.PropertyFactory.textFont
import org.maplibre.android.style.layers.PropertyFactory.textHaloColor
import org.maplibre.android.style.layers.PropertyFactory.textHaloWidth
import org.maplibre.android.style.layers.PropertyFactory.textIgnorePlacement
import org.maplibre.android.style.layers.PropertyFactory.textOffset
import org.maplibre.android.style.layers.PropertyFactory.textSize
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonOptions
import org.maplibre.android.style.sources.GeoJsonSource
import uniffi.apgo_ffi.LineKind
import uniffi.apgo_ffi.lineWidthCurveBase
import uniffi.apgo_ffi.lineWidthStops

// How things look on the map. Sizes are in map pixels, opacities 0..1, icon sizes are factors of the bitmap.
// Line widths scale with zoom and come from the core (lineWidthStops), so iOS draws them the same.
private const val REALM_FILL_OPACITY = 0.07f

// Area quests fill in as they progress: empty until started, strongest while in progress, faint once done.
private const val AREA_FILL_PROGRESS = 0.22f
private const val AREA_FILL_DONE = 0.1f

// A park's outline: thin, in its state colour, always dashed (in line widths: dash, gap).
private val PARK_DASH = arrayOf(3f, 2f)

// A route (trail): its state colour on a white casing; done routes fade.
private const val DONE_ROUTE_OPACITY = 0.5f

// The selected quest is marked on the ground too: a small ring at its pin's point.
private const val QUEST_HALO_RADIUS = 9f
private const val QUEST_HALO_STROKE = 3f
private const val DRAFT_FILL_OPACITY = 0.15f
private const val DRAFT_DOT_RADIUS = 5f
private const val DRAFT_DOT_STROKE = 1.5f
private const val MARK_RADIUS = 12f
private const val MARK_STROKE = 3f
private const val HANDLE_RADIUS = 11f
private const val HANDLE_STROKE = 3.5f
private const val KNOB_RADIUS = 6f
private const val KNOB_STROKE = 3f
private const val LABEL_SIZE = 14f
private const val LABEL_HALO_WIDTH = 2.5f
private const val LABEL_RAISE = -0.3f
private const val ME_PIN_PX = 120
private const val HOME_PIN_PX = 168
private const val ME_SIZE = 0.75f
private const val HOME_SIZE = 0.8f
private const val ROUND = "round"
private const val BADGE_ME = "badge-me"
private const val BADGE_HOME = "marker-home"
private const val GEOMETRY_POINT = "Point"
private const val GEOMETRY_POLYGON = "Polygon"

// Pins are full size from street level in and shrink to half by neighbourhood zoom: zooming out shrinks them first.
private const val FULL_SIZE_ZOOM = 16f
private const val SHRUNK_ZOOM = 13f
private const val SHRUNK_FACTOR = 0.5f

// Then, still too close, they collapse: pins nearer than this (map pixels) merge into a cluster. Clusters only form below
// FULL_SIZE_ZOOM, so at street level every pin shows on its own, big.
private const val CLUSTER_RADIUS = 16
private const val CLUSTER_MAX_ZOOM = 15
private const val CLUSTER_CIRCLE_RADIUS = 20f
private const val CLUSTER_STROKE = 3f
private const val CLUSTER_TEXT_SIZE = 15f
private const val RING_STEPS = 12
private const val BOLD_FONT = "Noto Sans Bold"

// The quest states drawn on the map, each in its ApgoPalette.quest colour.
private val PIN_STATES = listOf("progress", "open", "locked", "done")

/** The names of the map's GeoJSON sources. */
internal object MapSource {
    const val REALMS = "realms"
    const val AREAS = "areas"
    const val LINES = "lines"
    const val TRACE = "trace"
    const val QUESTS = "quests"
    const val QUEST_SEL = "quest-sel"
    const val FINDS = "finds"
    const val FIND_SEL = "find-sel"
    const val DRAFT = "draft"
    const val MARKS = "marks"
    const val HOME = "home"
    const val HANDLES = "handles"
    const val RADIUS = "radius"
    const val RING_KNOBS = "ringknobs"
    const val RING_LABEL = "ringlabel"
    const val ME = "me"
    val ALL =
        listOf(
            REALMS,
            AREAS,
            LINES,
            TRACE,
            QUESTS,
            QUEST_SEL,
            FINDS,
            FIND_SEL,
            DRAFT,
            MARKS,
            HOME,
            HANDLES,
            RADIUS,
            RING_KNOBS,
            RING_LABEL,
            ME,
        )

    /** Pin sources that merge pins too close to tell apart into one numbered circle. */
    val CLUSTERED = setOf(QUESTS, FINDS)
}

/** The base map and the sources and layers drawn over it. */
internal object MapStyle {
    const val URL = "https://tiles.openfreemap.org/styles/liberty"

    /** The layers a tap on a find pin is looked up in. */
    val FIND_LAYERS = arrayOf("finds-layer", "finds-sel")

    /** The layers a tap on a quest pin is looked up in. */
    val QUEST_LAYERS = arrayOf("quests-pins", "quests-pins-sel")

    /** Then the quests' trails and park outlines (both carry their quest's id). */
    val QUEST_LINE_LAYERS = arrayOf("route-line", "route-casing", "park-line")

    /** The cluster circle layer of each clustered source: a tap on one zooms in until it splits. */
    val CLUSTER_LAYERS = MapSource.CLUSTERED.associateBy { clusterLayer(it) }

    /** Add every source and layer to a freshly loaded style, in drawing order. */
    fun install(style: Style) {
        BaseMap.trim(style)
        val empty = GeoJson.collection(emptyList())
        MapSource.ALL.forEach { id ->
            style.addSource(if (id in MapSource.CLUSTERED) GeoJsonSource(id, empty, clusterOptions()) else GeoJsonSource(id, empty))
        }
        addLayers(style, realmLayers() + traceLayers() + questLayers() + findLayers() + draftLayers() + markLayers())
        addLayers(style, radiusLayers())
        // You and home are badges: a person on blue, a house on green.
        style.addImage(BADGE_ME, renderPin(ApgoIcons.Me, ME_PIN_PX, fill = ApgoPalette.me))
        style.addImage(BADGE_HOME, renderMarker(ApgoIcons.Home, HOME_PIN_PX, ApgoPalette.home))
        // You first, then home on top: when they are in the same spot the house is the one you see.
        addLayers(
            style,
            listOf(
                badgeLayer("me-layer", MapSource.ME, BADGE_ME, ME_SIZE),
                badgeLayer("home-layer", MapSource.HOME, BADGE_HOME, HOME_SIZE),
            ),
        )
    }

    private fun addLayers(
        style: Style,
        layers: List<Layer>,
    ) = layers.forEach(style::addLayer)

    private fun selectedOnly() = Expression.eq(Expression.get(MapProp.SELECTED), Expression.literal(true))

    private fun stateColor() =
        Expression.match(
            Expression.get(MapProp.STATE),
            Expression.literal(ApgoPalette.questTodo.hex()),
            *PIN_STATES.map { Expression.stop(it, Expression.literal(ApgoPalette.quest(it).hex())) }.toTypedArray(),
        )

    private fun realmLayers() =
        listOf(
            FillLayer("realms-fill", MapSource.REALMS).withProperties(fillColor(ApgoPalette.realm.hex()), fillOpacity(REALM_FILL_OPACITY)),
            LineLayer(
                "realms-line",
                MapSource.REALMS,
            ).withProperties(lineColor(ApgoPalette.realm.hex()), scaledWidth(LineKind.REALM_OUTLINE)),
            FillLayer("areas-fill", MapSource.AREAS).withProperties(fillColor(stateColor()), fillOpacity(areaFillOpacity())),
        )

    private fun areaFillOpacity() =
        Expression.match(
            Expression.get(MapProp.STATE),
            Expression.literal(0f),
            Expression.stop("progress", Expression.literal(AREA_FILL_PROGRESS)),
            Expression.stop("done", Expression.literal(AREA_FILL_DONE)),
        )

    private fun traceLayers() =
        listOf(
            LineLayer("trace-layer", MapSource.TRACE).withProperties(
                lineColor(ApgoPalette.trace.hex()),
                scaledWidth(LineKind.TRACE),
                lineCap(ROUND),
                lineJoin(ROUND),
            ),
            // Parks: a thin dashed outline; only its state colour tells the state, as on pins. The fill shows progress (realmLayers).
            LineLayer("park-line", MapSource.LINES)
                .withFilter(isPark())
                .withProperties(lineColor(stateColor()), scaledWidth(LineKind.PARK_OUTLINE), lineDasharray(PARK_DASH)),
            // Trails and other routes: a line in the state colour on a white casing, so it never looks like the base map's own
            // dashed paths. Direction does not matter (coverage counts either way). Done routes fade.
            LineLayer("route-casing", MapSource.LINES)
                .withFilter(Expression.not(isPark()))
                .withProperties(
                    lineColor(ApgoPalette.onMap.hex()),
                    scaledWidth(LineKind.TRAIL_CASING),
                    lineOpacity(routeOpacity()),
                    lineCap(ROUND),
                    lineJoin(ROUND),
                ),
            LineLayer("route-line", MapSource.LINES)
                .withFilter(Expression.not(isPark()))
                .withProperties(
                    lineColor(stateColor()),
                    scaledWidth(LineKind.TRAIL),
                    lineOpacity(routeOpacity()),
                    lineCap(ROUND),
                    lineJoin(ROUND),
                ),
        )

    private fun isPoint() = Expression.eq(Expression.geometryType(), Expression.literal(GEOMETRY_POINT))

    private fun isPark() = Expression.eq(Expression.get(MapProp.SHAPE), Expression.literal("area"))

    private fun isDone() = Expression.eq(Expression.get(MapProp.STATE), Expression.literal("done"))

    private fun routeOpacity() = Expression.switchCase(isDone(), Expression.literal(DONE_ROUTE_OPACITY), Expression.literal(1f))

    // Quests are the same pins as finds (family colour, state as a badge). Every pin shows; pins too close to tell apart merge into
    // a numbered cluster. The selected one has its own source (never clustered), a halo, and is drawn larger.
    private fun questLayers() =
        questClusterLayers() +
            listOf(
                SymbolLayer("quests-pins", MapSource.QUESTS).withFilter(notCluster()).withProperties(
                    iconImage(Expression.get(MapProp.IMAGE)),
                    iconSize(shrinkWhenZoomedOut(Expression.literal(MapMarkers.QUEST_SCALE))),
                    iconAllowOverlap(true),
                    iconAnchor(Property.ICON_ANCHOR_BOTTOM),
                    symbolSortKey(Expression.get(MapProp.SORT)),
                ),
                CircleLayer("quests-sel", MapSource.QUEST_SEL).withProperties(
                    circleRadius(QUEST_HALO_RADIUS),
                    circleColor(ApgoPalette.onMap.hex()),
                    circleStrokeColor(ApgoPalette.navy.hex()),
                    circleStrokeWidth(QUEST_HALO_STROKE),
                ),
                SymbolLayer("quests-pins-sel", MapSource.QUEST_SEL).withProperties(
                    iconImage(Expression.get(MapProp.IMAGE)),
                    iconSize(MapMarkers.QUEST_SCALE * MapMarkers.SELECTED_GROWTH),
                    iconAllowOverlap(true),
                    iconAnchor(Property.ICON_ANCHOR_BOTTOM),
                    iconIgnorePlacement(true),
                ),
            )

    private fun clusterOptions() =
        GeoJsonOptions()
            .withCluster(true)
            .withClusterRadius(CLUSTER_RADIUS)
            .withClusterMaxZoom(CLUSTER_MAX_ZOOM)
            .apply {
                // How many pins of each state a cluster holds, for its ring (finds have no state, so theirs stay 0).
                MapMarkers.RING_STATES.forEach { state ->
                    val isState = Expression.eq(Expression.get(MapProp.STATE), Expression.literal(state))
                    withClusterProperty(
                        countProp(state),
                        Expression.literal("+"),
                        Expression.switchCase(isState, Expression.literal(1), Expression.literal(0)),
                    )
                }
            }

    // A pin size that is [full] from FULL_SIZE_ZOOM in, easing down to SHRUNK_FACTOR of it at SHRUNK_ZOOM and below. (A zoom
    // expression must be the input of a top-level interpolate, so the factor goes inside each stop.)
    private fun shrinkWhenZoomedOut(full: Expression) =
        Expression.interpolate(
            Expression.linear(),
            Expression.zoom(),
            Expression.stop(SHRUNK_ZOOM, Expression.product(full, Expression.literal(SHRUNK_FACTOR))),
            Expression.stop(FULL_SIZE_ZOOM, full),
        )

    // A line width the renderer scales with zoom every frame (GPU side; no camera listener), from the core's stops.
    private fun scaledWidth(kind: LineKind) =
        lineWidth(
            Expression.interpolate(
                Expression.exponential(lineWidthCurveBase()),
                Expression.zoom(),
                *lineWidthStops(kind).map { Expression.stop(it.zoom, it.width) }.toTypedArray(),
            ),
        )

    private fun countProp(state: String) = "n_$state"

    private fun clusterLayer(source: String) = "$source-cluster"

    private fun isCluster() = Expression.has(MapProp.POINT_COUNT)

    private fun notCluster() = Expression.not(isCluster())

    // The ring image of a quest cluster: "ring|p|o|l|d", each state's share in twelfths, rounded up so even one in-progress quest
    // shows. Few distinct keys come out of that, and each is drawn once, when the map first asks for it (see MarkerSpec.Ring).
    private fun ringKey(): Expression {
        val parts = MapMarkers.RING_STATES.flatMap { listOf(Expression.literal("|"), Expression.toString(Expression.ceil(ringShare(it)))) }
        return Expression.concat(Expression.literal("ring"), *parts.toTypedArray())
    }

    private fun ringShare(state: String) =
        Expression.division(
            Expression.product(Expression.get(countProp(state)), Expression.literal(RING_STEPS)),
            Expression.get(MapProp.POINT_COUNT),
        )

    // A quest cluster is a ring split by state (amber in progress, blue open, grey locked, green done) around a dark count.
    private fun questClusterLayers() =
        listOf(
            SymbolLayer(clusterLayer(MapSource.QUESTS), MapSource.QUESTS).withFilter(isCluster()).withProperties(
                iconImage(ringKey()),
                iconAllowOverlap(true),
                iconIgnorePlacement(true),
            ),
            countLayer(MapSource.QUESTS, ApgoPalette.navy),
        )

    // A find cluster is a plain teal disc with a white count: finds have no progress to show.
    private fun findClusterLayers() =
        listOf(
            CircleLayer(clusterLayer(MapSource.FINDS), MapSource.FINDS).withFilter(isCluster()).withProperties(
                circleRadius(CLUSTER_CIRCLE_RADIUS),
                circleColor(ApgoPalette.teal.hex()),
                circleStrokeColor(ApgoPalette.onMap.hex()),
                circleStrokeWidth(CLUSTER_STROKE),
            ),
            countLayer(MapSource.FINDS, ApgoPalette.onMap),
        )

    private fun countLayer(
        source: String,
        color: Color,
    ) = SymbolLayer("$source-count", source).withFilter(isCluster()).withProperties(
        textField(Expression.toString(Expression.get(MapProp.POINT_COUNT))),
        textFont(arrayOf(BOLD_FONT)),
        textSize(CLUSTER_TEXT_SIZE),
        textColor(color.hex()),
        textAllowOverlap(true),
        textIgnorePlacement(true),
    )

    // Finds: every pin shows, favorites drawn over plain ones and banned ones; close pins merge into a neutral cluster (finds have
    // no progress to show). The selected find has its own unclustered source and is drawn larger.
    private fun findLayers() =
        findClusterLayers() +
            listOf(
                SymbolLayer(FIND_LAYERS[0], MapSource.FINDS).withFilter(notCluster()).withProperties(
                    iconImage(Expression.get(MapProp.IMAGE)),
                    iconSize(shrinkWhenZoomedOut(Expression.literal(MapMarkers.FIND_SIZE))),
                    iconAllowOverlap(true),
                    iconAnchor(Property.ICON_ANCHOR_BOTTOM),
                    symbolSortKey(Expression.get(MapProp.SORT)),
                    iconOpacity(Expression.get(MapProp.OPACITY)),
                ),
                SymbolLayer(FIND_LAYERS[1], MapSource.FIND_SEL).withProperties(
                    iconImage(Expression.get(MapProp.IMAGE)),
                    iconSize(MapMarkers.FIND_SELECTED_SIZE),
                    iconAllowOverlap(true),
                    iconAnchor(Property.ICON_ANCHOR_BOTTOM),
                    iconIgnorePlacement(true),
                ),
            )

    // The draft source holds the outline and its corner dots: each layer takes only the geometry it can draw.
    private fun draftLayers() =
        listOf(
            LineLayer("draft-line", MapSource.DRAFT)
                .withFilter(Expression.not(isPoint()))
                .withProperties(lineColor(ApgoPalette.draft.hex()), scaledWidth(LineKind.DRAFT)),
            FillLayer("draft-fill", MapSource.DRAFT)
                .withFilter(Expression.eq(Expression.geometryType(), Expression.literal(GEOMETRY_POLYGON)))
                .withProperties(fillColor(ApgoPalette.draft.hex()), fillOpacity(DRAFT_FILL_OPACITY)),
            CircleLayer("draft-pts", MapSource.DRAFT)
                .withFilter(isPoint())
                .withProperties(
                    circleRadius(DRAFT_DOT_RADIUS),
                    circleColor(ApgoPalette.draft.hex()),
                    circleStrokeColor(ApgoPalette.onMap.hex()),
                    circleStrokeWidth(DRAFT_DOT_STROKE),
                ),
        )

    // The thaw point and detour waypoint, and the handles the player can drag.
    private fun markLayers() =
        listOf(
            CircleLayer("marks-layer", MapSource.MARKS).withProperties(
                circleRadius(MARK_RADIUS),
                circleColor(Expression.get(MapProp.COLOR)),
                circleStrokeColor(ApgoPalette.onMap.hex()),
                circleStrokeWidth(MARK_STROKE),
            ),
            CircleLayer("handles-layer", MapSource.HANDLES).withProperties(
                circleRadius(HANDLE_RADIUS),
                circleColor(ApgoPalette.onMap.hex()),
                circleStrokeColor(ApgoPalette.draft.hex()),
                circleStrokeWidth(HANDLE_STROKE),
            ),
        )

    // The circle's radius: a line from the centre to the ring with the value above it, and a grip knob on the ring (the whole ring is
    // draggable).
    private fun radiusLayers() =
        listOf(
            LineLayer(
                "radius-line",
                MapSource.RADIUS,
            ).withProperties(lineColor(ApgoPalette.draftStrong.hex()), scaledWidth(LineKind.RADIUS_RING)),
            CircleLayer("ringknobs-layer", MapSource.RING_KNOBS).withProperties(
                circleRadius(KNOB_RADIUS),
                circleColor(ApgoPalette.onMap.hex()),
                circleStrokeColor(ApgoPalette.draft.hex()),
                circleStrokeWidth(KNOB_STROKE),
            ),
            SymbolLayer("ringlabel-layer", MapSource.RING_LABEL).withProperties(
                textField(Expression.get(MapProp.LABEL)),
                textFont(arrayOf(BOLD_FONT)),
                textSize(LABEL_SIZE),
                textColor(ApgoPalette.draftStrong.hex()),
                textHaloColor(ApgoPalette.onMap.hex()),
                textHaloWidth(LABEL_HALO_WIDTH),
                textAllowOverlap(true),
                textIgnorePlacement(true),
                textAnchor(Property.TEXT_ANCHOR_BOTTOM),
                textOffset(arrayOf(0f, LABEL_RAISE)),
            ),
        )

    private fun badgeLayer(
        id: String,
        source: String,
        image: String,
        size: Float,
    ) = SymbolLayer(id, source).withProperties(iconImage(image), iconSize(size), iconAllowOverlap(true), iconIgnorePlacement(true))
}
