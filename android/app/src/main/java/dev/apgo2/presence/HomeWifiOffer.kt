package dev.apgo2.presence

import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/** A position in degrees with its accuracy in metres (0 for a pin). */
internal data class GeoFix(
    val lat: Double,
    val lon: Double,
    val accuracyM: Double,
)

/** What [HomeWifiOffer.decide] looks at. */
internal data class OfferSignals(
    val saved: List<HomeNetwork>,
    val playing: Boolean,
    val fix: GeoFix?,
    val home: GeoFix?,
    val wifi: WifiId?,
    val muted: Set<String>,
    val showing: Boolean,
    val dismissedAtMs: Long?,
    val nowMs: Long,
)

/**
 * "You're home: add this Wi-Fi?" A player who skipped home Wi-Fi in setup never gets the at-home pause; when a good fix puts them at
 * the home pin while on a named network, offer to save that network. Pure.
 */
internal object HomeWifiOffer {
    const val NEAR_HOME_M = 75.0
    const val MAX_ACCURACY_M = 50.0
    const val LATER_COOLDOWN_MS = 10 * 60 * 1000L
    private const val EARTH_RADIUS_M = 6_371_000.0

    /** The network to offer (SSID cleaned), or `null` when no offer should appear now. */
    fun decide(s: OfferSignals): WifiId? {
        val net = PresenceSignals.usableNetwork(s.wifi) ?: return null
        val fix = s.fix
        val home = s.home
        val cooling = s.dismissedAtMs != null && s.nowMs - s.dismissedAtMs < LATER_COOLDOWN_MS
        val ok =
            s.saved.isEmpty() && s.playing && !s.showing && !cooling && net.ssid !in s.muted &&
                fix != null && home != null && fix.accuracyM <= MAX_ACCURACY_M && distanceM(fix, home) <= NEAR_HOME_M
        return if (ok) net else null
    }

    /** Great-circle distance in metres (haversine). */
    fun distanceM(
        a: GeoFix,
        b: GeoFix,
    ): Double {
        val dLat = Math.toRadians(b.lat - a.lat)
        val dLon = Math.toRadians(b.lon - a.lon)
        val h = sin(dLat / 2).let { it * it } + cos(Math.toRadians(a.lat)) * cos(Math.toRadians(b.lat)) * sin(dLon / 2).let { it * it }
        return 2 * EARTH_RADIUS_M * asin(min(1.0, sqrt(h)))
    }
}
