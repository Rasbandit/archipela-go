package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class PipLayoutTest {
    private val eps = 1e-4f

    private fun assertPoints(
        expected: List<Pair<Float, Float>>,
        actual: List<Pair<Float, Float>>,
    ) {
        assertEquals(expected.size, actual.size)
        expected.zip(actual).forEach { (e, a) ->
            assertEquals(e.first, a.first, eps)
            assertEquals(e.second, a.second, eps)
        }
    }

    @Test fun oneDotSitsOnTheMiddleLine() {
        assertPoints(listOf(100f to 50f), PipLayout.centers(1, cx = 100f, top = 50f, gap = 20f))
    }

    @Test fun twoDotsSitSideBySide() {
        assertPoints(listOf(90f to 50f, 110f to 50f), PipLayout.centers(2, cx = 100f, top = 50f, gap = 20f))
    }

    @Test fun threeDotsFormADownPointingTriangle() {
        val c = PipLayout.centers(3, cx = 100f, top = 50f, gap = 20f)
        // Two on top, one centred below at the height of an equilateral triangle.
        assertPoints(listOf(90f to 50f, 110f to 50f, 100f to 50f + 20f * 0.8660254f), c)
    }

    @Test fun noneOrTooManyAreClamped() {
        assertEquals(emptyList<Pair<Float, Float>>(), PipLayout.centers(0, 100f, 50f, 20f))
        assertEquals(3, PipLayout.centers(5, 100f, 50f, 20f).size)
    }
}
