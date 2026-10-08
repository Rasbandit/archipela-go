package dev.apgo2

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.apgo_ffi.FindOut
import uniffi.apgo_ffi.GeoPoint

class EditorFindsTest {
    private val saved = mutableListOf<Triple<String, String, String>>()
    private var saveWorks = true

    private fun find(
        id: String,
        mark: String = "none",
    ) = FindOut(id, id, true, emptyList(), emptyList(), "bench", "rest", GeoPoint(1.0, 2.0), 0.0, mark)

    private fun editorFinds(vararg finds: FindOut) =
        EditorFinds({ finds.toList() }, { rid, fid, mark -> saveWorks.also { if (it) saved += Triple(rid, fid, mark) } })

    @Test fun loadReplacesTheFindsAndBumpsTheVersion() {
        val f = editorFinds(find("a"), find("b"))
        runBlocking { f.load("r1") }
        runBlocking { f.load("r1") }
        assertEquals(listOf("a", "b"), f.all.map { it.id })
        assertEquals(2, f.version)
    }

    @Test fun markingTwiceTheSameWayClearsTheMark() {
        val f = editorFinds(find("a"))
        runBlocking { f.load("r1") }
        f.mark("r1", f.all[0], FindFilter.FAVORITE)
        assertEquals(FindFilter.FAVORITE, f.all[0].mark)
        f.mark("r1", f.all[0], FindFilter.FAVORITE)
        assertEquals("none", f.all[0].mark)
        assertEquals(listOf(Triple("r1", "a", FindFilter.FAVORITE), Triple("r1", "a", "none")), saved)
        assertEquals(3, f.version)
    }

    @Test fun aMarkThatCannotBeSavedChangesNothing() {
        val f = editorFinds(find("a"))
        runBlocking { f.load("r1") }
        saveWorks = false
        f.mark("r1", f.all[0], FindFilter.BANNED)
        assertEquals("none", f.all[0].mark)
        assertEquals(1, f.version)
    }

    @Test fun showSelectsAndAsksForANewFocusEachTime() {
        val f = editorFinds()
        f.show(find("a"), density = 2f)
        assertEquals("a", f.selected)
        assertEquals(1, f.focus?.nonce)
        assertEquals(230 * 2 + 26 * 2, f.focus?.roomAbovePx)
        f.show(find("a"), density = 2f)
        assertEquals(2, f.focus?.nonce)
    }

    @Test fun refocusUsesTheMeasuredCalloutOnlyOnceItIsKnown() {
        val f = editorFinds(find("a"))
        runBlocking { f.load("r1") }
        f.refocusOnBubble(density = 1f)
        assertNull(f.focus)
        f.selected = "a"
        f.refocusOnBubble(density = 1f)
        assertNull(f.focus)
        f.bubblePx = 100
        f.refocusOnBubble(density = 1f)
        assertEquals(100 + 26, f.focus?.roomAbovePx)
    }
}
