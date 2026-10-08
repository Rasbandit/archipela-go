package dev.apgo2

import uniffi.apgo_ffi.QuestOut

/** Which quests go in the "Progress" section at the top of Play and which in the "On the map" list below the map. */
object PlayLayout {
    /** Quests with no spot on the map (steps, new squares, time away): the list is the only place they can be seen. */
    private val OFF_MAP = setOf("steps", "cells", "away")
    private val STATE_ORDER = listOf("progress", "open", "locked", "done", "hidden")

    data class Split(val progress: List<QuestOut>, val places: List<QuestOut>)

    fun split(quests: List<QuestOut>): Split {
        val (top, rest) = quests.partition { q ->
            q.state == "progress" || (q.shape in OFF_MAP && q.state == "open")
        }
        return Split(
            progress = top.sortedByDescending { it.progress },
            places = rest.sortedBy { STATE_ORDER.indexOf(it.state) },
        )
    }
}
