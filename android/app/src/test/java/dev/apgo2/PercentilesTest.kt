package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class PercentilesTest {
    @Test fun nearestRankPercentiles() {
        val v = (1L..100L).toList().shuffled()
        assertEquals(50L, Percentiles.of(v, 0.5))
        assertEquals(99L, Percentiles.of(v, 0.99))
        assertEquals(0L, Percentiles.of(emptyList(), 0.5))
    }
}
