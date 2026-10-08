package dev.apgo2.ui

import java.util.Locale

/** Distances and areas in the units the player's region uses: miles in the US, UK and a few others, kilometres elsewhere. */
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

    private fun trim(v: Double) =
        when {
            v >= HUNDREDS -> "%.0f".format(v)
            v >= TENS -> "%.1f".format(v)
            else -> "%.2f".format(v)
        }

    fun distance(m: Double): String =
        if (imperial()) {
            if (m <
                SHORT_MILE_FRACTION * M_PER_MILE
            ) {
                "%.0f ft".format(m / M_PER_FOOT)
            } else {
                "${trim(m / M_PER_MILE).trimEnd('0').trimEnd('.')} mi"
            }
        } else {
            if (m < M_PER_KM) "%.0f m".format(m) else "${trim(m / M_PER_KM).trimEnd('0').trimEnd('.')} km"
        }

    fun area(m2: Double): String = if (imperial()) "${trim(m2 / (M_PER_MILE * M_PER_MILE))} mi²" else "${trim(m2 / M2_PER_KM2)} km²"
}
