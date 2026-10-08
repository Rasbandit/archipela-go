package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.apgo_ffi.ChainOut
import uniffi.apgo_ffi.MarkOut

private const val EPS = 1e-4f

class ChainFormatTest {
    private fun chain(
        unit: String,
        counter: Double,
        vararg marks: Pair<Double, Boolean>,
    ) = ChainOut(
        id = "1:x",
        zone = 1u,
        kindId = "x",
        name = "X",
        family = "steps",
        unit = unit,
        counter = counter,
        total = marks.last().first,
        rule = "r",
        marks = marks.mapIndexed { i, (at, reached) -> MarkOut(at = at, locationId = i.toLong(), reached = reached, reward = null) },
    )

    @Test fun amountsReadNaturallyPerUnit() {
        assertEquals("8,500 steps", ChainFormat.amount("steps", 8500.0))
        assertEquals("1 h 30 min", ChainFormat.amount("minutes", 90.0))
        assertEquals("45 min", ChainFormat.amount("minutes", 45.0))
        assertEquals("40 squares", ChainFormat.amount("cells", 40.0))
        assertEquals("1,234,567", ChainFormat.thousands(1_234_567))
    }

    @Test fun nextNamesTheFirstUnreachedMarkAndHowFarItIs() {
        val c = chain("steps", 3_400.0, 500.0 to true, 3_000.0 to true, 8_500.0 to false, 16_500.0 to false)
        assertEquals("next: 8,500 steps (5,100 to go)", ChainFormat.next(c))
    }

    @Test fun aFinishedChainSaysSo() {
        val c = chain("steps", 9_000.0, 500.0 to true, 8_500.0 to true)
        assertEquals("all 2 unlocked", ChainFormat.next(c))
    }

    @Test fun ticksSitAtTheirShareOfTheTotalAndStayInsideTheBar() {
        assertEquals(listOf(0.1f, 0.5f, 1.0f), ChainFormat.fractions(listOf(10.0, 50.0, 100.0), 100.0))
        assertEquals(listOf(1.0f), ChainFormat.fractions(listOf(250.0), 100.0))
        assertEquals(emptyList<Float>(), ChainFormat.fractions(emptyList(), 100.0))
        assertEquals("a zero total must not divide by zero", listOf(0f), ChainFormat.fractions(listOf(5.0), 0.0))
    }

    @Test fun ticksKeepAMinimumGapSoNeighboursDoNotOverlap() {
        // 500 and 1,000 on a 30,000 bar sit at 1.7% and 3.3%: too close to tell apart.
        val f = ChainFormat.fractions(listOf(500.0, 1_000.0, 30_000.0), 30_000.0)
        assertEquals(3, f.size)
        f.zipWithNext().forEach { (a, b) -> assertTrue("gap $a -> $b", b - a >= ChainFormat.MIN_TICK_GAP - EPS) }
        assertEquals("the first tick is not moved", 500f / 30_000f, f[0], EPS)
        assertEquals("the last tick stays at the end", 1f, f[2], EPS)
    }

    @Test fun ticksCrowdedAtTheEndArePushedBackInsideTheBar() {
        val f = ChainFormat.fractions(listOf(98.0, 99.0, 100.0), 100.0)
        assertEquals(1f, f[2], EPS)
        f.zipWithNext().forEach { (a, b) -> assertTrue("gap $a -> $b", b - a >= ChainFormat.MIN_TICK_GAP - EPS) }
        f.forEach { assertTrue("inside the bar: $it", it in 0f..1f) }
    }

    @Test fun tooManyTicksForTheGapAreSpreadEvenly() {
        val n = (1 / ChainFormat.MIN_TICK_GAP).toInt() + 5
        val f = ChainFormat.fractions(List(n) { 100.0 }, 100.0)
        assertEquals(0f, f.first(), EPS)
        assertEquals(1f, f.last(), EPS)
        f.zipWithNext().forEach { (a, b) -> assertEquals(1f / (n - 1), b - a, EPS) }
    }

    @Test fun theFillIsTheCounterShareClamped() {
        assertEquals(0.25f, ChainFormat.fill(25.0, 100.0))
        assertEquals(1f, ChainFormat.fill(500.0, 100.0))
        assertEquals(0f, ChainFormat.fill(-3.0, 100.0))
        assertEquals(0f, ChainFormat.fill(10.0, 0.0))
    }
}
