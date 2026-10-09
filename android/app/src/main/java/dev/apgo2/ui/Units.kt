package dev.apgo2.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.apgo_ffi.UnitSystem
import uniffi.apgo_ffi.formatArea
import uniffi.apgo_ffi.formatDistance
import java.util.Locale

/**
 * Distances and areas in the player's units ([system], resolved by the core from the unit setting and the phone's region). The
 * formatting itself is the core's (core/src/units.rs), so the app, iOS and the core's own quest text always agree. Numbers use
 * a decimal point and Western digits whatever the phone's locale: every distance and percentage the player sees goes through here.
 */
internal object Units {
    /** The units to show; Compose state, so every distance on screen redraws when the setting changes. Set by UnitSettings. */
    var system by mutableStateOf(UnitSystem.METRIC)

    private const val PERCENT = 100

    /** A distance in metres; a NaN or infinite one (bad geometry) reads "–" rather than crashing the screen. */
    fun distance(m: Double): String = formatDistance(m, system)

    /** A share in 0..1 as a whole percentage, e.g. "38%"; float noise below 0 reads "0%", not "-0%". */
    fun percent(fraction: Double): String = "%.0f%%".format(Locale.US, fraction.coerceAtLeast(0.0) * PERCENT)

    /** An area in square metres: km² or mi², one decimal under 10. */
    fun area(m2: Double): String = formatArea(m2, system)
}
