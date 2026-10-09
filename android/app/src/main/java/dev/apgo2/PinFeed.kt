package dev.apgo2

import android.hardware.GeomagneticField
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.maplibre.android.geometry.LatLng

// Compass readings reach the core at most twice a second, and the debug raw track once a second.
private const val HEADING_MIN_MS = 500L
private const val RAW_HEADING_MS = 1_000L

/** Where the player is for the app's logic (ruling T15-me): the simulator, else the estimate (a game is open), else the raw fix. */
internal object Here {
    fun pick(
        sim: LatLng?,
        estimate: LatLng?,
        raw: LatLng?,
    ): LatLng? = sim ?: estimate ?: raw

    /**
     * Where the player is for game logic (Archipelago traps placed around them, adversarial review M3): the simulator, else the last
     * accepted estimate. Never the pin, which may be bridged, predicted or stale, nor an unvetted raw fix.
     */
    fun logic(
        sim: LatLng?,
        accepted: LatLng?,
    ): LatLng? = sim ?: accepted
}

/** The open game's map pin from the core's `position()`, and the compass readings the core turns into its arrow. */
internal class PinFeed(
    private val model: AppModel,
) {
    /** The pin the Play map shows; null with no game open or before its first fix. */
    var pin by mutableStateOf<MePin?>(null)
        private set
    private val headingThrottle = Throttle(HEADING_MIN_MS)
    private val rawHeadingThrottle = Throttle(RAW_HEADING_MS)

    /** Ask the core where to draw the player now (after every fix, step batch and compass reading). */
    fun refresh() {
        val e = model.engine
        pin = if (e.hasGame()) model.now().let { t -> e.position(t)?.let { MePins.from(it, pin, t) } } else null
    }

    /** A compass reading: a magnetic one corrected to true north with the declination here; sent at most twice a second. */
    fun onHeading(r: CompassReading) {
        if (!model.engine.hasGame() || !headingThrottle.due(r.eventMs)) return
        val at = model.here ?: return
        val declination = if (r.trueNorth) 0.0 else declinationDeg(at, r.eventMs)
        val h = CompassReadings.headingIn(r, declination)
        val raw = RawLines.heading(h.azimuthDeg, h.accuracy, h.pitchDeg, h.rollDeg, h.tMs, h.errorDeg)
        if (rawHeadingThrottle.due(r.eventMs)) Diag.raw("rawhead", raw)
        model.engine.onHeading(h)
        refresh()
    }

    private fun declinationDeg(
        at: LatLng,
        tMs: Long,
    ): Double = GeomagneticField(at.latitude.toFloat(), at.longitude.toFloat(), 0f, tMs).declination.toDouble()
}
