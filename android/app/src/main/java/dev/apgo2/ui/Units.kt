package dev.apgo2.ui

import java.util.Locale

/**
 * Distances and areas in the units the player's region uses: miles in the US, UK and a few others, kilometres elsewhere.
 * Numbers always use Locale.US (a decimal point), like the rest of the app (ChainFormat.thousands).
 */
internal object Units {
    private const val M_PER_MILE = 1609.344
    private const val M_PER_FOOT = 0.3048
    private const val M_PER_KM = 1000
    private const val M2_PER_KM2 = 1_000_000.0
    private const val SHORT_MILE_FRACTION = 0.1
    private const val HUNDREDS = 100
    private const val TENS = 10
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

    fun distance(m: Double): String =
        if (imperial()) {
            if (m <
                SHORT_MILE_FRACTION * M_PER_MILE
            ) {
                "${fmt("%.0f", m / M_PER_FOOT)} ft"
            } else {
                "${short(m / M_PER_MILE)} mi"
            }
        } else {
            if (m < M_PER_KM) "${fmt("%.0f", m)} m" else "${short(m / M_PER_KM)} km"
        }

    fun area(m2: Double): String = if (imperial()) "${trim(m2 / (M_PER_MILE * M_PER_MILE))} mi²" else "${trim(m2 / M2_PER_KM2)} km²"
}
