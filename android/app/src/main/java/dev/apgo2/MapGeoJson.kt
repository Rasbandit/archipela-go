package dev.apgo2

import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.METERS_PER_DEGREE
import dev.apgo2.ui.MapMarkers
import dev.apgo2.ui.MarkerSpec
import dev.apgo2.ui.Units
import dev.apgo2.ui.circleRing
import dev.apgo2.ui.hex
import org.json.JSONArray
import org.json.JSONObject
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.RealmOut
import kotlin.math.cos

private const val MIN_POLYGON_POINTS = 3
private const val BANNED_OPACITY = 0.55
private const val HIDDEN = "hidden"

/** The names of the properties that map features carry and the style reads. */
internal object MapProp {
    const val STATE = "state"
    const val SELECTED = "sel"
    const val IMAGE = "img"
    const val SORT = "z"
    const val OPACITY = "op"
    const val ID = "id"
    const val COLOR = "color"
    const val LABEL = "label"
    const val NAME = "name"

    /** On a quest line: "line" (a trail or other route) or "area" (a park's outline). */
    const val SHAPE = "shape"

    /** On a cluster (set by MapLibre): how many pins it holds. */
    const val POINT_COUNT = "point_count"
}

/** Builders for the GeoJSON the map sources are fed. */
internal object GeoJson {
    private const val TYPE = "type"
    private const val COORDINATES = "coordinates"

    /** All [features] as one feature collection, as text. */
    fun collection(features: List<JSONObject>): String =
        JSONObject()
            .put(TYPE, "FeatureCollection")
            .put("features", JSONArray(features))
            .toString()

    /** A feature with [geometry] and [props]. */
    fun feature(
        geometry: JSONObject,
        props: JSONObject = JSONObject(),
    ): JSONObject =
        JSONObject()
            .put(TYPE, "Feature")
            .put("geometry", geometry)
            .put("properties", props)

    /** A point feature. */
    fun pointFeature(
        lat: Double,
        lon: Double,
        props: JSONObject = JSONObject(),
    ): JSONObject = feature(JSONObject().put(TYPE, "Point").put(COORDINATES, coord(lat, lon)), props)

    /** A line through [points] (lat, lon). */
    fun lineString(points: List<Pair<Double, Double>>): JSONObject =
        JSONObject()
            .put(TYPE, "LineString")
            .put(COORDINATES, JSONArray(points.map { coord(it.first, it.second) }))

    /** A filled shape with [points] (lat, lon) as its outline; the outline is closed here. */
    fun polygon(points: List<Pair<Double, Double>>): JSONObject {
        val ring = JSONArray()
        points.forEach { ring.put(coord(it.first, it.second)) }
        points.firstOrNull()?.let { ring.put(coord(it.first, it.second)) }
        return JSONObject()
            .put(TYPE, "Polygon")
            .put(COORDINATES, JSONArray().put(ring))
    }

    private fun coord(
        lat: Double,
        lon: Double,
    ) = JSONArray().put(lon).put(lat)
}

/** The name of this quest's pin image in the map style. */
internal val QuestOut.mapImageKey: String get() = MarkerSpec.Quest(kindId, family, state, MapMarkers.pips(difficulty, boss)).key

/** The name of this find's pin image in the map style. */
internal val MapFind.mapImageKey: String get() = MarkerSpec.Find(kindId, family, mark).key

/** The radius line, ring knob and value label of a circle being edited. */
internal data class RadiusFeatures(
    val line: List<JSONObject>,
    val knobs: List<JSONObject>,
    val label: List<JSONObject>,
)

/** The features each map source shows, made from the game state. */
internal object MapFeatures {
    /** Quest pins: anchored quests that are not lines, and the second stop of couriers. */
    fun quests(
        quests: List<QuestOut>,
        selected: Long?,
    ): List<JSONObject> {
        val visible = quests.filter { it.state != HIDDEN }
        val anchored = visible.filter { it.shape != "line" }.mapNotNull { q -> q.anchor?.let { q to it } }
        val dropOffs = visible.filter { it.shape == "courier" }.mapNotNull { q -> q.anchorB?.let { q to it } }
        return (anchored + dropOffs).map { (q, p) -> GeoJson.pointFeature(p.lat, p.lon, questProps(q, q.locationId == selected)) }
    }

    /** [pins] split into the rest (a clustered source) and the selected ones (their own source, so they never vanish into a cluster). */
    fun splitSelected(pins: List<JSONObject>): Pair<List<JSONObject>, List<JSONObject>> =
        pins.partition { !it.getJSONObject("properties").optBoolean(MapProp.SELECTED) }

    /** The routes of line and area quests. */
    fun lines(quests: List<QuestOut>): List<JSONObject> =
        quests.filter { it.state != HIDDEN && it.path.size >= 2 && (it.shape == "line" || it.shape == "area") }.map {
            val props = JSONObject().put(MapProp.STATE, it.state).put(MapProp.SHAPE, it.shape)
            GeoJson.feature(GeoJson.lineString(it.path.map { p -> p.lat to p.lon }), props)
        }

    /** The outlines of area quests. */
    fun areas(quests: List<QuestOut>): List<JSONObject> =
        quests.filter { it.state != HIDDEN && it.shape == "area" && it.path.size >= MIN_POLYGON_POINTS }.map {
            GeoJson.feature(GeoJson.polygon(it.path.map { p -> p.lat to p.lon }), JSONObject().put(MapProp.STATE, it.state))
        }

    /** Where you have been, one line per unbroken stretch of GPS. */
    fun trace(trace: List<List<LatLng>>): List<JSONObject> =
        trace.filter { it.size >= 2 }.map { seg ->
            GeoJson.feature(GeoJson.lineString(seg.map { it.latitude to it.longitude }))
        }

    /** The outline of each realm, circle or polygon, whichever is active. */
    fun realms(realms: List<RealmOut>): List<JSONObject> =
        realms.mapNotNull { r ->
            val pts =
                if (r.polygonActive) {
                    r.polygon.takeIf { it.size >= MIN_POLYGON_POINTS }?.map { it.lat to it.lon }
                } else {
                    r.circle?.let { circleRing(it.center.lat, it.center.lon, it.radiusM) }
                }
            pts?.let { GeoJson.feature(GeoJson.polygon(it), JSONObject().put(MapProp.NAME, r.name)) }
        }

    /** The shape being drawn: the circle or the polygon (or the line of its first two corners), and the corner dots when editable. */
    fun draft(
        draft: List<LatLng>,
        circle: Pair<LatLng, Double>?,
        editable: Boolean,
    ): List<JSONObject> {
        val out = mutableListOf<JSONObject>()
        if (circle != null) {
            val (c, r) = circle
            out += GeoJson.feature(GeoJson.polygon(circleRing(c.latitude, c.longitude, r)))
            if (editable) out += GeoJson.pointFeature(c.latitude, c.longitude) // the centre handle's dot
        }
        val corners = draft.map { it.latitude to it.longitude }
        when {
            draft.size >= MIN_POLYGON_POINTS -> out += GeoJson.feature(GeoJson.polygon(corners))
            draft.size == 2 -> out += GeoJson.feature(GeoJson.lineString(corners))
        }
        if (editable) draft.forEach { out += GeoJson.pointFeature(it.latitude, it.longitude) } // the corner dots
        return out
    }

    /** Find pins: favorites sort first and banned last, banned are faded. */
    fun finds(finds: List<MapFind>): List<JSONObject> =
        finds.map {
            val sortKey =
                when (it.mark) {
                    "favorite" -> 0
                    "banned" -> 2
                    else -> 1
                }
            val opacity = if (it.mark == "banned") BANNED_OPACITY else 1.0
            val props =
                JSONObject()
                    .put(MapProp.ID, it.id)
                    .put(MapProp.IMAGE, it.mapImageKey)
                    .put(MapProp.SELECTED, it.selected)
                    .put(MapProp.SORT, sortKey)
                    .put(MapProp.OPACITY, opacity)
            GeoJson.pointFeature(it.at.latitude, it.at.longitude, props)
        }

    /** The thaw point and the detour waypoint, each in its own colour. */
    fun marks(
        thaw: LatLng?,
        waypoint: LatLng?,
    ): List<JSONObject> =
        listOfNotNull(thaw?.to(ApgoPalette.thaw.hex()), waypoint?.to(ApgoPalette.waypoint.hex())).map { (p, color) ->
            GeoJson.pointFeature(p.latitude, p.longitude, JSONObject().put(MapProp.COLOR, color))
        }

    /** A single pin at [p], or none. */
    fun pin(p: LatLng?): List<JSONObject> = listOfNotNull(p?.let { GeoJson.pointFeature(it.latitude, it.longitude) })

    /** The radius line, ring knob and value label of the circle being edited; null when there is nothing to show. */
    fun radius(
        circle: Pair<LatLng, Double>?,
        editable: Boolean,
    ): RadiusFeatures? =
        circle?.takeIf { editable }?.let { (c, r) ->
            val dLon = r / (METERS_PER_DEGREE * cos(Math.toRadians(c.latitude)))
            val text = Units.distance(r)
            val ring = c.longitude + dLon
            RadiusFeatures(
                line = listOf(GeoJson.feature(GeoJson.lineString(listOf(c.latitude to c.longitude, c.latitude to ring)))),
                knobs = listOf(GeoJson.pointFeature(c.latitude, ring)),
                label = listOf(GeoJson.pointFeature(c.latitude, c.longitude + dLon / 2, JSONObject().put(MapProp.LABEL, text))),
            )
        }

    // What a quest pin needs on the map: its image, draw order, id and whether it is the selected one.
    private fun questProps(
        q: QuestOut,
        selected: Boolean,
    ) = JSONObject()
        .put(MapProp.STATE, q.state)
        .put(MapProp.SELECTED, selected)
        .put(MapProp.IMAGE, q.mapImageKey)
        .put(MapProp.SORT, MapMarkers.drawOrder(q.state))
        .put(MapProp.ID, q.locationId.toString())
}
