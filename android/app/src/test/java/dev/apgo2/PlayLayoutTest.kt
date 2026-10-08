package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.apgo_ffi.QuestOut

class PlayLayoutTest {
    private fun q(id: Long, shape: String, state: String = "open", progress: Float = 0f) = QuestOut(
        locationId = id, zone = 1u, name = "q$id", place = "", family = "", kindId = "", difficulty = "Easy", tier = 1u, effortMin = 10.0,
        mode = "walk", state = state, progress = progress, shape = shape, anchor = null, anchorB = null, radiusM = 0.0, path = emptyList(),
        detail = "", fallback = false, boss = false, blurb = "", reward = null,
    )

    private fun ids(l: List<QuestOut>) = l.map { it.locationId }

    @Test fun questsWithNoPlaceOnTheMapGoToTheProgressSection() {
        val s = PlayLayout.split(listOf(q(1, "steps"), q(2, "cells"), q(3, "away"), q(4, "point"), q(5, "line")))
        assertEquals(setOf(1L, 2L, 3L), ids(s.progress).toSet())
        assertEquals(setOf(4L, 5L), ids(s.places).toSet())
    }

    @Test fun aMapQuestJoinsTheProgressSectionWhileItIsInProgress() {
        val s = PlayLayout.split(listOf(q(1, "line", "progress", 0.4f), q(2, "point", "progress", 0.0f), q(3, "dwell")))
        assertEquals(setOf(1L, 2L), ids(s.progress).toSet())
        assertEquals(listOf(3L), ids(s.places))
    }

    @Test fun doneLockedAndHiddenQuestsStayOutOfTheProgressSection() {
        val s = PlayLayout.split(listOf(q(1, "steps", "done", 1f), q(2, "steps", "locked"), q(3, "cells", "hidden"), q(4, "steps")))
        assertEquals(listOf(4L), ids(s.progress))
        assertEquals(setOf(1L, 2L, 3L), ids(s.places).toSet())
    }

    @Test fun progressIsSortedByHowCloseToDoneAndPlacesByState() {
        val s = PlayLayout.split(listOf(q(1, "steps", progress = 0.1f), q(2, "cells", "progress", 0.9f), q(3, "away", "progress", 0.5f)))
        assertEquals(listOf(2L, 3L, 1L), ids(s.progress))
        val p = PlayLayout.split(listOf(q(1, "point", "done"), q(2, "point", "open"), q(3, "point", "locked"), q(4, "point", "hidden")))
        assertEquals(listOf(2L, 3L, 1L, 4L), ids(p.places))
    }
}
