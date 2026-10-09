package dev.apgo2

import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
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
import org.maplibre.android.style.sources.GeoJsonSource

// How things look on the map. Sizes are in map pixels, opacities 0..1, icon sizes are factors of the bitmap.
private const val REALM_FILL_OPACITY = 0.07f
private const val REALM_LINE_WIDTH = 1.8f

// Area quests fill in as they progress: empty until started, strongest while in progress, faint once done.
private const val AREA_FILL_PROGRESS = 0.22f
private const val AREA_FILL_DONE = 0.1f

// Not-started routes and outlines are dashed (in line widths: dash, gap) so state reads without relying on colour alone.
private val NOT_STARTED_DASH = arrayOf(2f, 1.5f)
private const val TRACE_WIDTH = 3f
private const val TRACE_OPACITY = 0.7f
private const val QUEST_LINE_WIDTH = 4f
private const val QUEST_HALO_RADIUS = 26f
private const val QUEST_HALO_STROKE = 3f
private const val SELECTED_PIN_GROWTH = 1.25f
private const val FIND_PIN_SIZE = 0.62f
private const val FIND_PIN_SELECTED_SIZE = 0.92f
private const val DRAFT_LINE_WIDTH = 3f
private const val DRAFT_FILL_OPACITY = 0.15f
private const val DRAFT_DOT_RADIUS = 5f
private const val DRAFT_DOT_STROKE = 1.5f
private const val MARK_RADIUS = 12f
private const val MARK_STROKE = 3f
private const val HANDLE_RADIUS = 11f
private const val HANDLE_STROKE = 3.5f
private const val RADIUS_LINE_WIDTH = 2.5f
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

/** The names of the map's GeoJSON sources. */
internal object MapSource {
    const val REALMS = "realms"
    const val AREAS = "areas"
    const val LINES = "lines"
    const val TRACE = "trace"
    const val QUESTS = "quests"
    const val FINDS = "finds"
    const val DRAFT = "draft"
    const val MARKS = "marks"
    const val HOME = "home"
    const val HANDLES = "handles"
    const val RADIUS = "radius"
    const val RING_KNOBS = "ringknobs"
    const val RING_LABEL = "ringlabel"
    const val ME = "me"
    val ALL =
        listOf(REALMS, AREAS, LINES, TRACE, QUESTS, FINDS, DRAFT, MARKS, HOME, HANDLES, RADIUS, RING_KNOBS, RING_LABEL, ME)
}

/** The base map and the sources and layers drawn over it. */
internal object MapStyle {
    const val URL = "https://tiles.openfreemap.org/styles/liberty"

    /** The layers a tap on a find pin is looked up in. */
    val FIND_LAYERS = arrayOf("finds-layer", "finds-sel")

    /** Add every source and layer to a freshly loaded style, in drawing order. */
    fun install(style: Style) {
        val empty = GeoJson.collection(emptyList())
        MapSource.ALL.forEach { style.addSource(GeoJsonSource(it, empty)) }
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
            Expression.stop("locked", Expression.literal(ApgoPalette.questLocked.hex())),
            Expression.stop("done", Expression.literal(ApgoPalette.questDone.hex())),
            Expression.stop("progress", Expression.literal(ApgoPalette.questProgress.hex())),
        )

    private fun realmLayers() =
        listOf(
            FillLayer("realms-fill", MapSource.REALMS).withProperties(fillColor(ApgoPalette.realm.hex()), fillOpacity(REALM_FILL_OPACITY)),
            LineLayer("realms-line", MapSource.REALMS).withProperties(lineColor(ApgoPalette.realm.hex()), lineWidth(REALM_LINE_WIDTH)),
            FillLayer("areas-fill", MapSource.AREAS).withProperties(fillColor(stateColor()), fillOpacity(areaFillOpacity())),
        )

    private fun areaFillOpacity() =
        Expression.match(
            Expression.get(MapProp.STATE),
            Expression.literal(0f),
            Expression.stop("progress", Expression.literal(AREA_FILL_PROGRESS)),
            Expression.stop("done", Expression.literal(AREA_FILL_DONE)),
        )

    // Open and locked quests have not been started. Two layers rather than a data-driven dash, which MapLibre Android does not
    // reliably support.
    private fun notStarted() =
        Expression.match(
            Expression.get(MapProp.STATE),
            Expression.literal(false),
            Expression.stop("open", Expression.literal(true)),
            Expression.stop("locked", Expression.literal(true)),
        )

    private fun traceLayers() =
        listOf(
            LineLayer("trace-layer", MapSource.TRACE).withProperties(
                lineColor(ApgoPalette.me.hex()),
                lineWidth(TRACE_WIDTH),
                lineOpacity(TRACE_OPACITY),
                lineCap(ROUND),
                lineJoin(ROUND),
            ),
            LineLayer("lines-layer", MapSource.LINES)
                .withFilter(Expression.not(notStarted()))
                .withProperties(lineColor(stateColor()), lineWidth(QUEST_LINE_WIDTH)),
            LineLayer("lines-dashed", MapSource.LINES)
                .withFilter(notStarted())
                .withProperties(lineColor(stateColor()), lineWidth(QUEST_LINE_WIDTH), lineDasharray(NOT_STARTED_DASH)),
        )

    // Quests are the same pins as finds (family colour, state as a badge). They thin out by collision, in-progress and open first;
    // the selected one gets a halo and is always drawn, larger.
    private fun questLayers() =
        listOf(
            CircleLayer("quests-sel", MapSource.QUESTS).withFilter(selectedOnly()).withProperties(
                circleRadius(QUEST_HALO_RADIUS),
                circleColor(ApgoPalette.onMap.hex()),
                circleStrokeColor(ApgoPalette.realm.hex()),
                circleStrokeWidth(QUEST_HALO_STROKE),
            ),
            SymbolLayer("quests-pins", MapSource.QUESTS).withProperties(
                iconImage(Expression.get(MapProp.IMAGE)),
                iconSize(Expression.get(MapProp.SCALE)),
                iconAllowOverlap(false),
                iconIgnorePlacement(false),
                symbolSortKey(Expression.get(MapProp.SORT)),
            ),
            SymbolLayer("quests-pins-sel", MapSource.QUESTS).withFilter(selectedOnly()).withProperties(
                iconImage(Expression.get(MapProp.IMAGE)),
                iconSize(Expression.product(Expression.get(MapProp.SCALE), Expression.literal(SELECTED_PIN_GROWTH))),
                iconAllowOverlap(true),
                iconIgnorePlacement(true),
            ),
        )

    // Finds: icon pins that thin out by collision, favorites winning over plain ones and banned ones; the selected find always shows.
    private fun findLayers() =
        listOf(
            SymbolLayer(FIND_LAYERS[0], MapSource.FINDS).withProperties(
                iconImage(Expression.get(MapProp.IMAGE)),
                iconSize(FIND_PIN_SIZE),
                iconAllowOverlap(false),
                iconIgnorePlacement(false),
                symbolSortKey(Expression.get(MapProp.SORT)),
                iconOpacity(Expression.get(MapProp.OPACITY)),
            ),
            SymbolLayer(FIND_LAYERS[1], MapSource.FINDS).withFilter(selectedOnly()).withProperties(
                iconImage(Expression.get(MapProp.IMAGE)),
                iconSize(FIND_PIN_SELECTED_SIZE),
                iconAllowOverlap(true),
                iconIgnorePlacement(true),
            ),
        )

    private fun draftLayers() =
        listOf(
            LineLayer("draft-line", MapSource.DRAFT).withProperties(lineColor(ApgoPalette.draft.hex()), lineWidth(DRAFT_LINE_WIDTH)),
            FillLayer("draft-fill", MapSource.DRAFT).withProperties(fillColor(ApgoPalette.draft.hex()), fillOpacity(DRAFT_FILL_OPACITY)),
            CircleLayer("draft-pts", MapSource.DRAFT)
                .withFilter(Expression.eq(Expression.geometryType(), Expression.literal(GEOMETRY_POINT)))
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
            ).withProperties(lineColor(ApgoPalette.draftStrong.hex()), lineWidth(RADIUS_LINE_WIDTH)),
            CircleLayer("ringknobs-layer", MapSource.RING_KNOBS).withProperties(
                circleRadius(KNOB_RADIUS),
                circleColor(ApgoPalette.onMap.hex()),
                circleStrokeColor(ApgoPalette.draft.hex()),
                circleStrokeWidth(KNOB_STROKE),
            ),
            SymbolLayer("ringlabel-layer", MapSource.RING_LABEL).withProperties(
                textField(Expression.get(MapProp.LABEL)),
                textFont(arrayOf("Noto Sans Bold")),
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
