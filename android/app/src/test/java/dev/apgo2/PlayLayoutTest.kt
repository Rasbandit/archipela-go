package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.apgo_ffi.QuestOut

class PlayLayoutTest {
    private fun quest(
        id: Long,
        shape: String,
        state: String = "open",
        progress: Float = 0f,
        chainId: String? = null,
    ) = QuestOut(
        locationId = id,
        zone = 1u,
        name = "q$id",
        place = "",
        family = "",
        kindId = "",
        difficulty = "Easy",
        tier = 1u,
        effortMin = 10.0,
        mode = "walk",
        state = state,
        progress = progress,
        shape = shape,
        anchor = null,
        anchorB = null,
        radiusM = 0.0,
        path = emptyList(),
        detail = "",
        fallback = false,
        boss = false,
        blurb = "",
        reward = null,
        chainId = chainId,
        collect = null,
    )

    private fun ids(l: List<QuestOut>) = l.map { it.locationId }

    @Test fun questsWithNoPlaceOnTheMapGoToTheProgressSection() {
        val s = PlayLayout.split(listOf(quest(1, "steps"), quest(2, "cells"), quest(3, "away"), quest(4, "point"), quest(5, "line")))
        assertEquals(setOf(1L, 2L, 3L), ids(s.progress).toSet())
        assertEquals(setOf(4L, 5L), ids(s.places).toSet())
    }

    @Test fun aMapQuestJoinsTheProgressSectionWhileItIsInProgress() {
        val s = PlayLayout.split(listOf(quest(1, "line", "progress", 0.4f), quest(2, "point", "progress", 0.0f), quest(3, "dwell")))
        assertEquals(setOf(1L, 2L), ids(s.progress).toSet())
        assertEquals(listOf(3L), ids(s.places))
    }

    @Test fun doneLockedAndHiddenQuestsStayOutOfTheProgressSection() {
        val s =
            PlayLayout.split(
                listOf(quest(1, "steps", "done", 1f), quest(2, "steps", "locked"), quest(3, "cells", "hidden"), quest(4, "steps")),
            )
        assertEquals(listOf(4L), ids(s.progress))
        assertEquals(setOf(1L, 2L, 3L), ids(s.places).toSet())
    }

    @Test fun chainMembersAreInNeitherListBecauseTheChainRowShowsThem() {
        val s =
            PlayLayout.split(
                listOf(
                    quest(1, "steps", chainId = "1:step_up"),
                    quest(2, "away", "progress", 0.4f, chainId = "1:wanderlust"),
                    quest(3, "point"),
                    quest(4, "steps"),
                ),
            )
        assertEquals(listOf(4L), ids(s.progress))
        assertEquals(listOf(3L), ids(s.places))
    }

    @Test fun progressIsSortedByHowCloseToDoneAndPlacesByState() {
        val s =
            PlayLayout.split(
                listOf(quest(1, "steps", progress = 0.1f), quest(2, "cells", "progress", 0.9f), quest(3, "away", "progress", 0.5f)),
            )
        assertEquals(listOf(2L, 3L, 1L), ids(s.progress))
        val p =
            PlayLayout.split(
                listOf(quest(1, "point", "done"), quest(2, "point", "open"), quest(3, "point", "locked"), quest(4, "point", "hidden")),
            )
        assertEquals(listOf(2L, 3L, 1L, 4L), ids(p.places))
    }
}
