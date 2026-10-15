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

    // Stores the name as given: naming a blank camp is the core's rule, tested there.
    override fun save(c: CampIn): String {
        val camp = CampOut(c.id.ifEmpty { "c${next++}" }, c.name, c.lat, c.lon)
        val at = saved.indexOfFirst { it.id == camp.id }
        if (at >= 0) saved[at] = camp else saved.add(camp)
        return camp.id
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

    @Test fun addingHandsTheCoreATrimmedNameAndReturnsTheIdItGave() {
        val id = book.add("  ", 40.0, -111.0)
        val c = store.saved.single()
        assertEquals("a blank name goes to the core, which picks the default", "", c.name)
        assertEquals(id, c.id)
        assertTrue(id.isNotEmpty())
        val cabin = book.add(" Cabin ", 1.0, 2.0)
        assertEquals("Cabin", store.saved.first { it.id == cabin }.name)
    }

    @Test fun renameAndMoveKeepTheIdAndDoNotAddACamp() {
        val id = book.add("Cabin", 1.0, 2.0)
        book.edit(id, "Lake house", 5.0, 6.0)
        assertEquals(CampOut(id, "Lake house", 5.0, 6.0), store.saved.single())
        book.edit(id, " Lake ", 5.0, 6.0)
        assertEquals("the name is trimmed", "Lake", store.saved.single().name)
    }

    @Test fun editingAnUnknownCampSavesNothingAndSaysSo() {
        assertFalse(book.edit("nope", "X", 1.0, 1.0))
        assertTrue(store.saved.isEmpty())
        val id = book.add("A", 1.0, 1.0)
        assertTrue(book.edit(id, "B", 1.0, 1.0))
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
        val rows = book.rows(store.camps(), GeoFix(40.0, -111.0, 5.0))
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
        assertNull(book.rows(store.camps(), null).single().distanceM)
    }

    @Test fun noCampsMeansNoRows() {
        assertTrue(book.rows(emptyList(), null).isEmpty())
    }
}
