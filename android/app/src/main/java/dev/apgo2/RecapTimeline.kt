package dev.apgo2

import kotlin.math.min

/** When each recap progress bar runs: one after another, each as long as its gain is big, landing softly. */
internal object RecapTimeline {
    const val FULL_BAR_MS = 2600L
    const val MIN_BAR_MS = 450L
    const val MAX_BAR_MS = 2400L
    const val GAP_MS = 380L
    const val START_MS = 450L

    /** The share of a bar's run spent easing to a stop. */
    const val LANDING = 0.12f

    /** One bar's run, in milliseconds from the start of the recap. */
    data class Slot(
        val startMs: Long,
        val durationMs: Long,
    )

    /** A slot per bar; [gainFractions] is each bar's gain as a share (0..1) of its full length. */
    fun slots(gainFractions: List<Float>): List<Slot> {
        var start = START_MS
        return gainFractions.map { f ->
            val duration = (f.coerceIn(0f, 1f) * FULL_BAR_MS).toLong().coerceIn(MIN_BAR_MS, MAX_BAR_MS)
            Slot(start, duration).also { start += duration + GAP_MS }
        }
    }

    /** When the last bar finishes (0 with no bars). */
    fun endMs(slots: List<Slot>): Long = slots.lastOrNull()?.let { it.startMs + it.durationMs } ?: 0L

    /** How far (0..1) a bar has filled [elapsedMs] into the recap. */
    fun progress(
        elapsedMs: Long,
        slot: Slot,
    ): Float = softLand(((elapsedMs - slot.startMs).toFloat() / slot.durationMs).coerceIn(0f, 1f))

    /** Linear, then easing to a stop over the last [tail] of [t] (0..1), ending exactly at 1. */
    fun softLand(
        t: Float,
        tail: Float = LANDING,
    ): Float {
        if (tail <= 0f) return t
        val v = 1f / (1f - tail / 2f)
        val u = t - (1f - tail)
        return if (t < 1f - tail) v * t else min(1f, v * (1f - tail) + v * (u - u * u / (2f * tail)))
    }
}
