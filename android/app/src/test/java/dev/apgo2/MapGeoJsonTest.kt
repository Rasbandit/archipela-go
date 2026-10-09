package dev.apgo2

import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.hex
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.CircleOut
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.RealmOut
import java.util.Locale

class GeoJsonTest {
    @Test fun aPointIsLonThenLat() {
        val f = GeoJson.pointFeature(40.5, -111.25, JSONObject().put("k", "v"))
        assertEquals("Feature", f.getString("type"))
        val g = f.getJSONObject("geometry")
        assertEquals("Point", g.getString("type"))
        assertEquals(-111.25, g.getJSONArray("coordinates").getDouble(0), 0.0)
        assertEquals(40.5, g.getJSONArray("coordinates").getDouble(1), 0.0)
        assertEquals("v", f.getJSONObject("properties").getString("k"))
    }

    @Test fun aFeatureWithoutPropsHasAnEmptyObject() {
        assertEquals(0, GeoJson.pointFeature(0.0, 0.0).getJSONObject("properties").length())
    }

    @Test fun aPolygonIsClosed() {
        val ring = GeoJson.polygon(listOf(1.0 to 2.0, 3.0 to 4.0, 5.0 to 6.0)).getJSONArray("coordinates").getJSONArray(0)
        assertEquals(4, ring.length())
        assertEquals(ring.getJSONArray(0).toString(), ring.getJSONArray(3).toString())
        assertEquals("lon first", 2.0, ring.getJSONArray(0).getDouble(0), 0.0)
        assertEquals(1.0, ring.getJSONArray(0).getDouble(1), 0.0)
    }

    @Test fun anEmptyPolygonHasAnEmptyRing() {
        val ring = GeoJson.polygon(emptyList()).getJSONArray("coordinates").getJSONArray(0)
        assertEquals(0, ring.length())
    }

    @Test fun aCollectionHoldsEveryFeature() {
        val c = JSONObject(GeoJson.collection(listOf(GeoJson.pointFeature(0.0, 0.0), GeoJson.pointFeature(1.0, 1.0))))
        assertEquals("FeatureCollection", c.getString("type"))
        assertEquals(2, c.getJSONArray("features").length())
        assertEquals(0, JSONObject(GeoJson.collection(emptyList())).getJSONArray("features").length())
    }

    @Test fun aLineKeepsItsOrder() {
        val coords = GeoJson.lineString(listOf(1.0 to 2.0, 3.0 to 4.0)).getJSONArray("coordinates")
        assertEquals(2, coords.length())
        assertEquals(4.0, coords.getJSONArray(1).getDouble(0), 0.0)
        assertEquals(3.0, coords.getJSONArray(1).getDouble(1), 0.0)
    }
}

class MapFeaturesTest {
    private val saved = Locale.getDefault()
    private val triangle = listOf(geo(0.0, 0.0), geo(0.0, 1.0), geo(1.0, 1.0))

    @After fun restore() = Locale.setDefault(saved)

    private fun geo(
        lat: Double,
        lon: Double,
    ) = GeoPoint(lat, lon)

    private fun quest(
        id: Long,
        shape: String = "point",
        state: String = "open",
        anchor: GeoPoint? = geo(1.0, 2.0),
        anchorB: GeoPoint? = null,
        path: List<GeoPoint> = emptyList(),
        difficulty: String = "Easy",
        boss: Boolean = false,
    ) = QuestOut(
        locationId = id,
        zone = 1u,
        name = "q$id",
        place = "",
        family = "reach",
        kindId = "street_smarts",
        difficulty = difficulty,
        tier = 1u,
        effortMin = 10.0,
        mode = "walk",
        state = state,
        progress = 0f,
        shape = shape,
        anchor = anchor,
        anchorB = anchorB,
        radiusM = 0.0,
        path = path,
        detail = "",
        fallback = false,
        boss = boss,
        blurb = "",
        reward = null,
        chainId = null,
    )

    private fun realm(
        name: String,
        circle: CircleOut? = null,
        polygon: List<GeoPoint> = emptyList(),
        polygonActive: Boolean = false,
    ) = RealmOut(
        id = name,
        name = name,
        icon = null,
        circle = circle,
        polygon = polygon,
        polygonActive = polygonActive,
        scannedAtMs = null,
        places = 0u,
        warning = null,
    )

    private fun JSONObject.props() = getJSONObject("properties")

    private fun JSONObject.geomType() = getJSONObject("geometry").getString("type")

    private fun JSONObject.coords(): JSONArray = getJSONObject("geometry").getJSONArray("coordinates")

    @Test fun questPinsSkipHiddenLinesAndUnanchoredQuests() {
        val pins =
            MapFeatures.quests(
                listOf(
                    quest(1),
                    quest(2, state = "hidden"),
                    quest(3, shape = "line"),
                    quest(4, anchor = null),
                ),
                selected = null,
            )
        assertEquals(1, pins.size)
        assertEquals("open", pins.single().props().getString(MapProp.STATE))
    }

    @Test fun aCourierGetsAPinAtBothStops() {
        val pins = MapFeatures.quests(listOf(quest(1, shape = "courier", anchorB = geo(5.0, 6.0))), selected = 1L)
        assertEquals(2, pins.size)
        assertEquals(6.0, pins[1].coords().getDouble(0), 0.0)
        pins.forEach { assertTrue(it.props().getBoolean(MapProp.SELECTED)) }
    }

    @Test fun questPinPropsCarryImageIdAndOrder() {
        val pins = MapFeatures.quests(listOf(quest(1, state = "progress", difficulty = "Hard"), quest(2, boss = true)), selected = 2L)
        val (hard, boss) = pins.map { it.props() }
        assertEquals("quest|street_smarts|reach|progress", hard.getString(MapProp.IMAGE))
        assertEquals("a tap on the pin finds its quest", "1", hard.getString(MapProp.ID))
        assertFalse(hard.getBoolean(MapProp.SELECTED))
        assertTrue(boss.getBoolean(MapProp.SELECTED))
        assertFalse("pins carry no size of their own: all are one size", boss.has("scale") || hard.has("scale"))
        assertTrue("in progress is drawn first", hard.getInt(MapProp.SORT) < boss.getInt(MapProp.SORT))
    }

    @Test fun theSelectedPinIsKeptOutOfTheClusteredSet() {
        val pins = MapFeatures.quests(listOf(quest(1), quest(2, shape = "courier", anchorB = geo(5.0, 6.0)), quest(3)), selected = 2L)
        val (rest, selected) = MapFeatures.splitSelected(pins)
        assertEquals("both courier stops stay with the selection", 2, selected.size)
        assertEquals(2, rest.size)
        rest.forEach { assertFalse(it.props().getBoolean(MapProp.SELECTED)) }
    }

    @Test fun nothingSelectedLeavesEveryPinClustered() {
        val (rest, selected) = MapFeatures.splitSelected(MapFeatures.quests(listOf(quest(1), quest(2)), selected = null))
        assertEquals(2, rest.size)
        assertTrue(selected.isEmpty())
        assertTrue(MapFeatures.splitSelected(emptyList()).let { it.first.isEmpty() && it.second.isEmpty() })
    }

    @Test fun linesAreDrawnForLineAndAreaQuestsWithAPath() {
        val two = listOf(geo(0.0, 0.0), geo(1.0, 1.0))
        val lines =
            MapFeatures.lines(
                listOf(
                    quest(1, shape = "line", path = two),
                    quest(2, shape = "area", path = triangle, state = "done"),
                    quest(3, shape = "line", path = listOf(geo(0.0, 0.0))),
                    quest(4, shape = "point", path = two),
                    quest(5, shape = "line", path = two, state = "hidden"),
                ),
            )
        assertEquals(listOf("open", "done"), lines.map { it.props().getString(MapProp.STATE) })
        assertEquals("parks and trails are drawn differently", listOf("line", "area"), lines.map { it.props().getString(MapProp.SHAPE) })
        lines.forEach { assertEquals("LineString", it.geomType()) }
    }

    @Test fun areasNeedThreeCorners() {
        val areas =
            MapFeatures.areas(
                listOf(
                    quest(1, shape = "area", path = triangle),
                    quest(2, shape = "area", path = triangle.take(2)),
                    quest(3, shape = "line", path = triangle),
                ),
            )
        assertEquals(1, areas.size)
        assertEquals("Polygon", areas.single().geomType())
    }

    @Test fun traceDropsSinglePointStretches() {
        val t = MapFeatures.trace(listOf(listOf(LatLng(0.0, 0.0)), listOf(LatLng(0.0, 0.0), LatLng(1.0, 1.0)), emptyList()))
        assertEquals(1, t.size)
        assertEquals(2, t.single().coords().length())
    }

    @Test fun realmsDrawTheActiveShape() {
        val circle = CircleOut(geo(10.0, 10.0), 100.0)
        val r =
            MapFeatures.realms(
                listOf(
                    realm("poly", circle = circle, polygon = triangle, polygonActive = true),
                    realm("circle", circle = circle, polygon = triangle),
                    realm("too-few", polygon = triangle.take(2), polygonActive = true),
                    realm("nothing"),
                ),
            )
        assertEquals(listOf("poly", "circle"), r.map { it.props().getString(MapProp.NAME) })
        assertEquals("triangle closed", 4, r[0].coords().getJSONArray(0).length())
        assertEquals("48-point ring closed", 49, r[1].coords().getJSONArray(0).length())
    }

    @Test fun draftShowsLineThenPolygonAndDotsWhenEditable() {
        val a = LatLng(0.0, 0.0)
        val b = LatLng(0.0, 1.0)
        val c = LatLng(1.0, 1.0)
        assertTrue(MapFeatures.draft(emptyList(), null, editable = true).isEmpty())
        assertEquals(listOf("Point"), MapFeatures.draft(listOf(a), null, editable = true).map { it.geomType() })
        assertTrue("one corner, not editable", MapFeatures.draft(listOf(a), null, editable = false).isEmpty())
        assertEquals(listOf("LineString"), MapFeatures.draft(listOf(a, b), null, editable = false).map { it.geomType() })
        assertEquals(
            listOf("Polygon", "Point", "Point", "Point"),
            MapFeatures.draft(listOf(a, b, c), null, editable = true).map { it.geomType() },
        )
    }

    @Test fun draftCircleHasACentreDotOnlyWhenEditable() {
        val circle = LatLng(0.0, 0.0) to 50.0
        assertEquals(listOf("Polygon", "Point"), MapFeatures.draft(emptyList(), circle, editable = true).map { it.geomType() })
        assertEquals(listOf("Polygon"), MapFeatures.draft(emptyList(), circle, editable = false).map { it.geomType() })
    }

    @Test fun findsSortFavoritesFirstAndFadeBanned() {
        fun find(mark: String) = MapFind(mark, LatLng(0.0, 0.0), "bench_warmer", "dwell", mark, selected = mark == "favorite")
        val props = MapFeatures.finds(listOf(find("favorite"), find("none"), find("banned"))).map { it.props() }
        assertEquals(listOf(0, 1, 2), props.map { it.getInt(MapProp.SORT) })
        assertEquals(listOf(1.0, 1.0, 0.55), props.map { it.getDouble(MapProp.OPACITY) })
        assertEquals(listOf(true, false, false), props.map { it.getBoolean(MapProp.SELECTED) })
        assertEquals("pin|bench_warmer|dwell|banned", props[2].getString(MapProp.IMAGE))
        assertEquals("favorite", props[0].getString(MapProp.ID))
    }

    @Test fun marksAreColouredAndMissingOnesLeftOut() {
        assertTrue(MapFeatures.marks(null, null).isEmpty())
        val both = MapFeatures.marks(LatLng(0.0, 0.0), LatLng(1.0, 1.0)).map { it.props().getString(MapProp.COLOR) }
        assertEquals(listOf(ApgoPalette.thaw.hex(), ApgoPalette.waypoint.hex()), both)
        val onlyWaypoint = MapFeatures.marks(null, LatLng(1.0, 1.0)).map { it.props().getString(MapProp.COLOR) }
        assertEquals(listOf(ApgoPalette.waypoint.hex()), onlyWaypoint)
    }

    @Test fun pinIsOneFeatureOrNone() {
        assertTrue(MapFeatures.pin(null).isEmpty())
        assertEquals(1, MapFeatures.pin(LatLng(1.0, 2.0)).size)
    }

    @Test fun radiusIsShownOnlyWhileEditing() {
        assertNull(MapFeatures.radius(null, editable = true))
        assertNull(MapFeatures.radius(LatLng(0.0, 0.0) to 100.0, editable = false))
    }

    private fun radiusLabel(r: Double) =
        requireNotNull(MapFeatures.radius(LatLng(0.0, 0.0) to r, editable = true))
            .label
            .single()
            .props()
            .getString(MapProp.LABEL)

    @Test fun radiusLabelUsesTheRegionsUnitWithADecimalPoint() {
        Locale.setDefault(Locale.GERMANY)
        assertEquals("999 m", radiusLabel(999.0))
        assertEquals("1 km", radiusLabel(999.9)) // was "999 m" (truncated); now rounded like every other distance
        assertEquals("1 km", radiusLabel(1000.0))
        assertEquals("2.5 km", radiusLabel(2500.0))
        Locale.setDefault(Locale.US)
        assertEquals("1.5 mi", radiusLabel(1.5 * 1609.344))
    }

    @Test fun radiusKnobSitsOnTheRingDueEastAndTheLabelHalfway() {
        val f = requireNotNull(MapFeatures.radius(LatLng(0.0, 10.0) to 111_195.0, editable = true))
        val knob = f.knobs.single().coords()
        assertEquals(11.0, knob.getDouble(0), 1e-9)
        assertEquals(0.0, knob.getDouble(1), 1e-9)
        assertEquals(
            10.5,
            f.label
                .single()
                .coords()
                .getDouble(0),
            1e-9,
        )
        val line = f.line.single().coords()
        assertEquals(10.0, line.getJSONArray(0).getDouble(0), 1e-9)
        assertEquals(11.0, line.getJSONArray(1).getDouble(0), 1e-9)
    }
}
