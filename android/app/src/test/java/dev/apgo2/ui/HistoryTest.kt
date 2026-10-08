package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HistoryTest {
    @Test fun aFreshHistoryHasNothingToUndoOrRedo() {
        val h = History("a")
        assertEquals("a", h.current)
        assertFalse(h.canUndo)
        assertFalse(h.canRedo)
        assertNull(h.undo())
        assertNull(h.redo())
        assertEquals("a", h.current)
    }

    @Test fun undoAndRedoWalkTheStates() {
        val h = History(1)
        h.push(2)
        h.push(3)
        assertTrue(h.canUndo)
        assertEquals(2, h.undo())
        assertEquals(1, h.undo())
        assertNull("at the start", h.undo())
        assertEquals(1, h.current)
        assertTrue(h.canRedo)
        assertEquals(2, h.redo())
        assertEquals(3, h.redo())
        assertNull("at the newest", h.redo())
        assertEquals(3, h.current)
    }

    @Test fun pushingTheCurrentStateRecordsNothing() {
        val h = History("a")
        h.push("a")
        assertFalse(h.canUndo)
        h.push("b")
        h.push("b")
        assertEquals("a", h.undo())
        assertFalse(h.canUndo)
    }

    @Test fun aNewEditForgetsTheRedoBranch() {
        val h = History(1)
        h.push(2)
        h.undo()
        h.push(3)
        assertFalse(h.canRedo)
        assertNull(h.redo())
        assertEquals(1, h.undo())
    }

    @Test fun theOldestStatesFallOffPastTheLimit() {
        val h = History(0, limit = 2)
        (1..4).forEach { h.push(it) }
        assertEquals(3, h.undo())
        assertEquals(2, h.undo())
        assertNull("0 and 1 were dropped", h.undo())
        assertEquals(2, h.current)
    }

    @Test fun resetMakesANewStartingPoint() {
        val h = History(1)
        h.push(2)
        h.push(3)
        h.undo()
        h.reset(9)
        assertEquals(9, h.current)
        assertFalse(h.canUndo)
        assertFalse(h.canRedo)
    }
}
