package dev.apgo2

import dev.apgo2.presence.GeoFix
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.apgo_ffi.CampIn
import uniffi.apgo_ffi.CampOut

private class FakeCamps : CampStore {
    val saved = mutableListOf<CampOut>()
    private var next = 0

    override fun camps() = saved.toList()

    override fun save(c: CampIn) {
        val camp = CampOut(c.id.ifEmpty { "c${next++}" }, c.name, c.lat, c.lon)
        val at = saved.indexOfFirst { it.id == camp.id }
        if (at >= 0) saved[at] = camp else saved.add(camp)
    }

    override fun delete(id: String) {
        saved.removeAll { it.id == id }
    }
}

private class FakeWifi : CampWifi {
    val byCamp = mutableMapOf<String, Int>()

    override fun networkCount(campId: String) = byCamp[campId] ?: 0

    override fun forget(campId: String) {
        byCamp.remove(campId)
    }
}

class CampBookTest {
    private val store = FakeCamps()
    private val wifi = FakeWifi()
    private val book = CampBook(store, wifi)

    @Test fun theFirstCampIsHomeThenCampNWithTheLowestFreeNumber() {
        assertEquals("Home", book.defaultName())
        book.add("", 1.0, 2.0)
        assertEquals("Camp 2", book.defaultName())
        book.add("Camp 2", 3.0, 4.0)
        assertEquals("Camp 3", book.defaultName())
        book.delete(store.saved[0].id)
        assertEquals("one camp left, Camp 2 is taken", "Camp 3", book.defaultName())
    }

    @Test fun addingWithABlankNameUsesTheDefaultAndAssignsAnId() {
        val id = book.add("  ", 40.0, -111.0)
        val c = store.saved.single()
        assertEquals("Home", c.name)
        assertEquals(id, c.id)
        assertTrue(id.isNotEmpty())
    }

    @Test fun renameAndMoveKeepTheIdAndDoNotAddACamp() {
        val id = book.add("Cabin", 1.0, 2.0)
        book.edit(id, "Lake house", 5.0, 6.0)
        assertEquals(CampOut(id, "Lake house", 5.0, 6.0), store.saved.single())
        book.edit(id, " ", 5.0, 6.0)
        assertEquals("a blank rename keeps the old name", "Lake house", store.saved.single().name)
    }

    @Test fun deletingACampForgetsItsWifiAndOnlyItsWifi() {
        val a = book.add("A", 1.0, 1.0)
        val b = book.add("B", 2.0, 2.0)
        wifi.byCamp[a] = 2
        wifi.byCamp[b] = 1
        book.delete(a)
        assertEquals(listOf(b), store.saved.map { it.id })
        assertEquals(0, wifi.networkCount(a))
        assertEquals(1, wifi.networkCount(b))
    }

    @Test fun rowsShowNameDistanceAndWifi() {
        val a = book.add("A", 40.0, -111.0)
        val b = book.add("B", 40.01, -111.0)
        wifi.byCamp[b] = 1
        val rows = book.rows(GeoFix(40.0, -111.0, 5.0))
        assertEquals(listOf("A", "B"), rows.map { it.name })
        assertEquals(0.0, rows[0].distanceM!!, 1.0)
        assertEquals(1_112.0, rows[1].distanceM!!, 5.0)
        assertFalse(rows[0].hasWifi)
        assertTrue(rows[1].hasWifi)
        assertEquals(a, rows[0].id)
        assertEquals(b, rows[1].id)
    }

    @Test fun withoutAFixThereIsNoDistance() {
        book.add("A", 1.0, 1.0)
        assertNull(book.rows(null).single().distanceM)
    }

    @Test fun noCampsMeansNoRows() {
        assertTrue(book.rows(null).isEmpty())
    }
}
