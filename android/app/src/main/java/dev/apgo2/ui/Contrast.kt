package dev.apgo2.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance

/** WCAG contrast between two colours, from 1 (same) to 21 (black on white): text needs 4.5, bars and icons 3. */
internal object Contrast {
    private const val FLARE = 0.05

    fun ratio(
        a: Color,
        b: Color,
    ): Double {
        val (hi, lo) = listOf(a.luminance().toDouble(), b.luminance().toDouble()).sortedDescending()
        return (hi + FLARE) / (lo + FLARE)
    }
}
