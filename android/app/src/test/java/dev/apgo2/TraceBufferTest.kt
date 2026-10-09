package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.TraceDelta
import uniffi.apgo_ffi.TrackSegmentOut

class TraceBufferTest {
    private fun line(vararg xs: Int) = TrackSegmentOut(xs.map { GeoPoint(it.toDouble(), 0.0) })

    private fun pts(vararg xs: Int) = xs.map { LatLng(it.toDouble(), 0.0) }

    private fun delta(
        cursor: Long,
        append: List<TrackSegmentOut>,
        tail: TrackSegmentOut = line(),
        reset: Boolean = false,
        joins: Boolean = false,
        fromMs: Long? = 5L,
    ) = TraceDelta(reset, cursor.toULong(), fromMs, joins, append, tail)

    @Test fun aResetReplacesEverythingAndTheTailIsDrawnLast() {
        val b = TraceBuffer()
        assertEquals(0UL, b.cursor)
        assertTrue("a first load", b.apply(delta(10, listOf(line(1, 2), line(5, 6)), line(6, 7), reset = true)))
        assertEquals(10UL, b.cursor)
        assertEquals(listOf(pts(1, 2), pts(5, 6), pts(6, 7)), b.lines())
        b.apply(delta(20, listOf(line(9, 10)), reset = true))
        assertEquals("the old lines are gone", listOf(pts(9, 10)), b.lines())
    }

    @Test fun deltasAppendToTheLastLineOrStartANewOneAndReplaceTheTail() {
        val b = TraceBuffer()
        b.apply(delta(10, listOf(line(1, 2)), line(2, 3), reset = true))
        assertFalse("the session start is unchanged", b.apply(delta(12, listOf(line(3, 4)), line(4, 5), joins = true)))
        assertEquals(listOf(pts(1, 2, 3, 4), pts(4, 5)), b.lines())
        b.apply(delta(15, listOf(line(5), line(8, 9)), line(), joins = true))
        assertEquals("joined, then a new line; no tail", listOf(pts(1, 2, 3, 4, 5), pts(8, 9)), b.lines())
        b.apply(delta(17, listOf(line(20, 21))))
        assertEquals(listOf(pts(1, 2, 3, 4, 5), pts(8, 9), pts(20, 21)), b.lines())
        b.apply(delta(17, emptyList(), line(21, 22)))
        assertEquals("nothing settled: only the tail changes", listOf(pts(1, 2, 3, 4, 5), pts(8, 9), pts(20, 21), pts(21, 22)), b.lines())
        assertEquals(17UL, b.cursor)
    }

    @Test fun joiningWithNothingHeldStartsALine() {
        val b = TraceBuffer()
        b.apply(delta(3, listOf(line(1, 2)), joins = true))
        assertEquals(listOf(pts(1, 2)), b.lines())
    }

    @Test fun theOlderTraceIsReloadedOnAResetOrWhileTheSessionStartIsUnknownOrMoves() {
        val b = TraceBuffer()
        assertTrue("no start yet: the journal still holds this session", b.apply(delta(1, emptyList(), fromMs = null, reset = true)))
        assertTrue(b.apply(delta(1, emptyList(), fromMs = null)))
        assertTrue("the start became known", b.apply(delta(2, listOf(line(1, 2)), fromMs = 5L)))
        assertFalse(b.apply(delta(3, listOf(line(3, 4)), fromMs = 5L)))
        assertTrue(b.apply(delta(4, emptyList(), fromMs = 9L)))
    }

    @Test fun clearForgetsTheLinesAndTheCursor() {
        val b = TraceBuffer()
        b.apply(delta(10, listOf(line(1, 2)), line(2, 3), reset = true))
        b.clear()
        assertEquals(0UL, b.cursor)
        assertEquals(emptyList<List<LatLng>>(), b.lines())
        assertTrue("after clearing, the older trace is reloaded", b.apply(delta(10, listOf(line(1, 2)), reset = true)))
    }
}
