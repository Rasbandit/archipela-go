package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class BubblePlacementTest {
    // A 1000 x 1600 map, a 300 x 200 bubble, 8 px margin, 26 px gap between pin and bubble.
    private fun place(x: Float, y: Float) = BubblePlacement.place(pinX = x, pinY = y, width = 300, height = 200, containerW = 1000, containerH = 1600, margin = 8, gap = 26)

    @Test fun sitsAbovePinCentredOnIt() {
        val (x, y) = place(500f, 800f)
        assertEquals(350, x) // 500 - 300/2
        assertEquals(800 - 200 - 26, y)
    }

    @Test fun movesBelowThePinWhenThereIsNoRoomAbove() {
        val (_, y) = place(500f, 100f)
        assertEquals(100 + 13, y) // pin + half the gap
    }

    @Test fun staysInsideTheScreenAtTheEdges() {
        assertEquals(8, place(10f, 800f).first)
        assertEquals(1000 - 300 - 8, place(990f, 800f).first)
    }

    @Test fun aBubbleWiderThanTheContainerStillHasAMargin() {
        val (x, _) = BubblePlacement.place(500f, 800f, width = 1200, height = 100, containerW = 1000, containerH = 1600, margin = 8, gap = 26)
        assertEquals(8, x)
    }
}
