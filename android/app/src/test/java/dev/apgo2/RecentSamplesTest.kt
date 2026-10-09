package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class RecentSamplesTest {
    @Test fun keepsOnlyTheNewestUpToTheCap() {
        val s = RecentSamples(3)
        (1L..5L).forEach(s::add)
        assertEquals(listOf(3L, 4L, 5L), s.values)
        s.clear()
        assertEquals(emptyList<Long>(), s.values)
    }
}
