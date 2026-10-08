package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class MapMarkersTest {
    @Test fun everyMarkerSurvivesAKeyRoundTrip() {
        val specs =
            listOf(
                MarkerSpec.Find("bench_warmer", "dwell", "none"),
                MarkerSpec.Find("hydrant_hunter", "landmark", "favorite"),
                MarkerSpec.Quest("street_smarts", "reach", "progress"),
                MarkerSpec.Quest("touch_grass", "park", "done"),
            )
        specs.forEach { assertEquals(it, MapMarkers.parse(it.key)) }
        assertEquals("keys are distinct", specs.size, specs.map { it.key }.toSet().size)
    }

    @Test fun anUnknownKeyParsesToNull() {
        assertNull(MapMarkers.parse("glyph|a|b"))
        assertNull(MapMarkers.parse("quest|only|two"))
        assertNull(MapMarkers.parse(""))
    }

    @Test fun eachQuestStateHasItsBadge() {
        assertEquals(MapMarkers.Badge.None, MapMarkers.badge("open"))
        assertEquals(MapMarkers.Badge.Progress, MapMarkers.badge("progress"))
        assertEquals(MapMarkers.Badge.Done, MapMarkers.badge("done"))
        assertEquals(MapMarkers.Badge.Locked, MapMarkers.badge("locked"))
        assertEquals(MapMarkers.Badge.None, MapMarkers.badge("something-new"))
    }

    @Test fun pinsGrowWithDifficultyAndTheBossIsBiggest() {
        val easy = MapMarkers.iconScale("easy", boss = false)
        val medium = MapMarkers.iconScale("medium", boss = false)
        val hard = MapMarkers.iconScale("Hard", boss = false)
        assertTrue(easy < medium && medium < hard)
        assertTrue(MapMarkers.iconScale("easy", boss = true) > hard)
        assertEquals("unknown difficulty reads as medium", medium, MapMarkers.iconScale("unknown", boss = false))
    }

    @Test fun inProgressAndOpenQuestsAreDrawnBeforeDoneOnes() {
        val order = listOf("progress", "open", "locked", "done").map { MapMarkers.drawOrder(it) }
        assertEquals(order.sorted(), order)
        assertEquals(order.toSet().size, order.size)
    }
}
