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
                MarkerSpec.Ring(listOf(1, 2, 0, 3)),
            )
        specs.forEach { assertEquals(it, MapMarkers.parse(it.key)) }
        assertEquals("keys are distinct", specs.size, specs.map { it.key }.toSet().size)
    }

    @Test fun anUnknownKeyParsesToNull() {
        assertNull(MapMarkers.parse("glyph|a|b"))
        assertNull(MapMarkers.parse("quest|only|two"))
        assertNull(MapMarkers.parse(""))
    }

    @Test fun aRingKeyNeedsOneNonNegativeSharePerStateAndSomethingToShow() {
        assertEquals(MarkerSpec.Ring(listOf(0, 4, 0, 0)), MapMarkers.parse("ring|0|4|0|0"))
        assertNull("nothing to draw", MapMarkers.parse("ring|0|0|0|0"))
        assertNull("one share per state", MapMarkers.parse("ring|1|1|1"))
        assertNull(MapMarkers.parse("ring|1|x|1|1"))
        assertNull(MapMarkers.parse("ring|1|-1|1|1"))
    }

    @Test fun ringStatesRunFromMostActionableToDone() {
        assertEquals(listOf("progress", "open", "locked", "done"), MapMarkers.RING_STATES)
    }

    @Test fun ringSegmentsSkipEmptySharesAndFillTheCircleInStateColours() {
        val segs = MapMarkers.ringSegments(MarkerSpec.Ring(listOf(1, 0, 1, 2)))
        assertEquals(listOf(ApgoPalette.questProgress, ApgoPalette.questLocked, ApgoPalette.questDone), segs.map { it.first })
        assertEquals(listOf(90f, 90f, 180f), segs.map { it.second })
        assertEquals(360f, MapMarkers.ringSegments(MarkerSpec.Ring(listOf(1, 1, 1, 0))).sumOf { it.second.toDouble() }.toFloat(), 0.01f)
    }

    @Test fun aRingWithOneStateIsAFullCircle() {
        assertEquals(listOf(ApgoPalette.questDone to 360f), MapMarkers.ringSegments(MarkerSpec.Ring(listOf(0, 0, 0, 7))))
    }

    @Test fun eachQuestStateHasItsBadge() {
        assertEquals(MapMarkers.Badge.None, MapMarkers.badge("open"))
        assertEquals(MapMarkers.Badge.Progress, MapMarkers.badge("progress"))
        assertEquals(MapMarkers.Badge.Done, MapMarkers.badge("done"))
        assertEquals(MapMarkers.Badge.Locked, MapMarkers.badge("locked"))
        assertEquals(MapMarkers.Badge.None, MapMarkers.badge("something-new"))
    }

    @Test fun everyQuestPinIsOneSizeBigEnoughToReadAndNeverUpscaled() {
        // Difficulty is not shown by size: a pin is a pin, so neighbours never look mismatched.
        assertTrue("a scale over 1 blurs the bitmap", MapMarkers.QUEST_SCALE <= 1f)
        assertTrue("a pin is drawn at least 96 px wide", MapMarkers.QUEST_SCALE * MapMarkers.QUEST_PIN_PX >= 96f)
    }

    @Test fun aQuestPinIsColouredByItsStateNotItsKind() {
        assertEquals(ApgoPalette.questTodo, MapMarkers.questFill("open"))
        assertEquals(ApgoPalette.questProgress, MapMarkers.questFill("progress"))
        assertEquals(ApgoPalette.questDone, MapMarkers.questFill("done"))
        assertEquals(ApgoPalette.muted, MapMarkers.questFill("locked"))
    }

    @Test fun pinsAreTallerThanWideSoTheirPointSitsBelowTheHead() {
        assertTrue(MapMarkers.PIN_HEIGHT_RATIO > 1f)
    }

    @Test fun aSelectedPinsHeightIsWhatACalloutMustClear() {
        val expected = MapMarkers.QUEST_PIN_PX * MapMarkers.PIN_HEIGHT_RATIO * MapMarkers.QUEST_SCALE * MapMarkers.SELECTED_GROWTH
        assertEquals(expected, MapMarkers.selectedQuestPinHeightPx(), 0.01f)
        assertTrue(MapMarkers.selectedFindPinHeightPx() > 0f)
    }

    @Test fun inProgressAndOpenQuestsAreDrawnBeforeDoneOnes() {
        val order = listOf("progress", "open", "locked", "done").map { MapMarkers.drawOrder(it) }
        assertEquals(order.sorted(), order)
        assertEquals(order.toSet().size, order.size)
    }
}
