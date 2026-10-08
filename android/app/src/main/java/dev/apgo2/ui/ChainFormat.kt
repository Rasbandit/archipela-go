package dev.apgo2.ui

import uniffi.apgo_ffi.ChainOut

private const val MINUTES_PER_HOUR = 60

/** Text and bar maths for a progressive quest (a chain). Pure, so it is unit-tested. */
internal object ChainFormat {
    /** The smallest gap between two ticks as a share of the bar: about 20 dp on a phone, more than a 12 dp tick. */
    const val MIN_TICK_GAP = 0.06f

    fun thousands(n: Long): String = "%,d".format(java.util.Locale.US, n)

    // The number with its unit word left off: "5,100", "1 h 30 min", "40".
    private fun bare(
        unit: String,
        value: Double,
    ): String =
        when (unit) {
            "steps" -> thousands(value.toLong())
            "minutes" -> minutes(value)
            else -> value.toLong().toString()
        }

    /** "8,500 steps", "1 h 30 min", "40 squares". */
    fun amount(
        unit: String,
        value: Double,
    ): String =
        when (unit) {
            "steps" -> "${bare(unit, value)} steps"
            "minutes" -> bare(unit, value)
            else -> "${bare(unit, value)} squares"
        }

    private fun minutes(m: Double): String {
        val total = m.coerceAtLeast(0.0).toLong()
        val (h, r) = total / MINUTES_PER_HOUR to total % MINUTES_PER_HOUR
        return when {
            h == 0L -> "$r min"
            r == 0L -> "$h h"
            else -> "$h h $r min"
        }
    }

    /** "next: 8,500 steps (5,100 to go)", or "all 4 unlocked" when every mark is reached. */
    fun next(c: ChainOut): String {
        val mark = c.marks.firstOrNull { !it.reached } ?: return "all ${c.marks.size} unlocked"
        val left = (mark.at - c.counter).coerceAtLeast(0.0)
        return "next: ${amount(c.unit, mark.at)} (${bare(c.unit, left)} to go)"
    }

    /**
     * Each mark's position along the bar, 0..1, at its share of the total but at least [MIN_TICK_GAP] from its
     * neighbours (marks are in order). Crowded ticks are pushed right, then back left from the end; when there are
     * too many for the gap they are spread evenly.
     */
    fun fractions(
        marks: List<Double>,
        total: Double,
    ): List<Float> {
        val f = marks.map { if (total <= 0.0) 0f else (it / total).toFloat().coerceIn(0f, 1f) }.toFloatArray()
        if (f.size < 2) return f.toList()
        val gap = minOf(MIN_TICK_GAP, 1f / (f.size - 1))
        for (i in 1 until f.size) f[i] = maxOf(f[i], f[i - 1] + gap)
        f[f.lastIndex] = minOf(f.last(), 1f)
        for (i in f.lastIndex - 1 downTo 0) f[i] = minOf(f[i], f[i + 1] - gap)
        return f.map { it.coerceIn(0f, 1f) }
    }

    fun fill(
        counter: Double,
        total: Double,
    ): Float = if (total <= 0.0) 0f else (counter / total).toFloat().coerceIn(0f, 1f)
}
