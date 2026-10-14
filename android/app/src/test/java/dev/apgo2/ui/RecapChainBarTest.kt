package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RecapChainBarTest {
    @Test
    fun markFractionClampsToCanvasRange() {
        assertEquals(0.03f, markFraction(0f, 5000f), 1e-6f)
        assertEquals(0.97f, markFraction(5000f, 5000f), 1e-6f)
        assertEquals(0.5f, markFraction(2500f, 5000f), 1e-6f)
        assertEquals(0.03f, markFraction(10f, 0f), 1e-6f)
    }

    @Test
    fun crossedListsNewlyReachedMarks() {
        val marks = listOf(1000f, 2500f, 5000f)
        assertEquals(listOf(1, 2), crossed(marks, 2140f, 6230f))
        assertEquals(emptyList<Int>(), crossed(marks, 2140f, 2140f))
        assertEquals(listOf(1), crossed(marks, 2140f, 2500f))
    }

    @Test
    fun confettiHasFourteenPaletteBits() {
        val tokens =
            setOf(
                ApgoPalette.questProgress,
                ApgoPalette.questDone,
                ApgoPalette.sky,
                ApgoPalette.butter,
                ApgoPalette.questTodo,
                ApgoPalette.grass,
            )
        assertEquals(14, CONFETTI.size)
        assertTrue(CONFETTI.all { it.color in tokens })
    }
}
