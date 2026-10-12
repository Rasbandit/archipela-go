package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import uniffi.apgo_ffi.CollectItemOut
import uniffi.apgo_ffi.CollectOut
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.QuestOut

class QuestMapKeyTest {
    private val path = listOf(GeoPoint(0.0, 0.0), GeoPoint(0.0, 1.0), GeoPoint(1.0, 1.0))

    private fun quest(
        state: String = "progress",
        shape: String = "area",
        progress: Float = 0.2f,
        text: String = "40 m to go",
    ) = QuestOut(
        locationId = 5,
        zone = 1u,
        name = "q",
        place = "",
        family = "reach",
        kindId = "street_smarts",
        difficulty = "Easy",
        tier = 1u,
        effortMin = 10.0,
        mode = "walk",
        state = state,
        progress = progress,
        shape = shape,
        anchor = GeoPoint(1.0, 2.0),
        anchorB = null,
        radiusM = 0.0,
        path = path,
        detail = text,
        fallback = false,
        boss = false,
        blurb = "",
        reward = null,
        chainId = null,
        collect = null,
        headline = text,
        cardDetail = text,
        placeRow = null,
        progressBlock = null,
        hasAbout = false,
    )

    @Test fun progressAndItsTextLeaveTheMapAsItIs() {
        val before = listOf(quest())
        val after = listOf(quest(progress = 0.4f, text = "20 m to go"))
        assertEquals(before.pinKey, after.pinKey)
        assertEquals(before.lineKey, after.lineKey)
    }

    @Test fun aNewStateRedrawsPinsAndLines() {
        val before = listOf(quest())
        val after = listOf(quest(state = "done"))
        assertNotEquals(before.pinKey, after.pinKey)
        assertNotEquals(before.lineKey, after.lineKey)
    }

    @Test fun aMovedPinRedrawsPinsButNotLines() {
        val before = listOf(quest())
        val after = listOf(quest().copy(anchor = GeoPoint(3.0, 4.0)))
        assertNotEquals(before.pinKey, after.pinKey)
        assertEquals(before.lineKey, after.lineKey)
    }

    @Test fun aNewRouteRedrawsLinesButNotPins() {
        val before = listOf(quest())
        val after = listOf(quest().copy(path = path.drop(1)))
        assertEquals(before.pinKey, after.pinKey)
        assertNotEquals(before.lineKey, after.lineKey)
    }

    @Test fun aPickedItemRedrawsThePickups() {
        val items = listOf(CollectItemOut(GeoPoint(1.0, 1.0), false), CollectItemOut(GeoPoint(2.0, 2.0), false))
        val forager = quest(shape = "collect").copy(collect = CollectOut("acorns", 2u, 0u, 0u, items))
        val picked = forager.copy(collect = forager.collect?.copy(items = listOf(items[0], items[1].copy(picked = true)), carried = 1u))
        assertNotEquals(listOf(forager).pinKey, listOf(picked).pinKey)
    }
}
