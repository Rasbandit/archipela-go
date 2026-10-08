package dev.apgo2.ui

import java.util.Locale
import kotlin.math.roundToLong

/**
 * Distances and areas in the units the player's region uses: miles in the US, UK and a few others, kilometres elsewhere.
 * Numbers are always formatted with Locale.US (a decimal point, Western digits), whatever the phone's locale: this is the
 * app's one number-locale policy, so every distance and percentage the player sees goes through here.
 */
internal object Units {
    private const val M_PER_MILE = 1609.344
    private const val M_PER_FOOT = 0.3048
    private const val NO_VALUE = "–"
    private const val M2_PER_KM2 = 1_000_000.0
    private const val SHORT_MILE_FRACTION = 0.1
    private const val HUNDREDS = 100
    private const val TENS = 10
    private const val PERCENT = 100
    private val imperialCountries = setOf("US", "GB", "LR", "MM")

    private fun imperial() = Locale.getDefault().country in imperialCountries

    private fun fmt(
        pattern: String,
        v: Double,
    ) = pattern.format(Locale.US, v)

    private fun trim(v: Double) =
        when {
            v >= HUNDREDS -> fmt("%.0f", v)
            v >= TENS -> fmt("%.1f", v)
            else -> fmt("%.2f", v)
        }

    // Drops trailing fractional zeros and a dangling point, never integer zeros: "1.50" -> "1.5", "100" stays "100".
    private fun short(v: Double) = trim(v).let { if ('.' in it) it.trimEnd('0').trimEnd('.') else it }

    /** A distance in metres; a NaN or infinite one (bad geometry) reads "–" rather than crashing the screen. */
    fun distance(m: Double): String =
        when {
            !m.isFinite() -> {
                NO_VALUE
            }

            imperial() -> {
                if (m < SHORT_MILE_FRACTION * M_PER_MILE) "${fmt("%.0f", m / M_PER_FOOT)} ft" else "${short(m / M_PER_MILE)} mi"
            }

            else -> {
                // Switch on the rounded value, or 999.6 m would read "1000 m".
                val whole = m.roundToLong()
                if (whole < METERS_PER_KM) "$whole m" else "${short(m / METERS_PER_KM)} km"
            }
        }

    /** A share in 0..1 as a whole percentage, e.g. "38%"; float noise below 0 reads "0%", not "-0%". */
    fun percent(fraction: Double): String = "${fmt("%.0f", fraction.coerceAtLeast(0.0) * PERCENT)}%"

    fun area(m2: Double): String = if (imperial()) "${trim(m2 / (M_PER_MILE * M_PER_MILE))} mi²" else "${trim(m2 / M2_PER_KM2)} km²"
}
