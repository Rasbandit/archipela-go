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
    /** How long ago [fix] was taken, on the monotonic clock (`elapsedRealtimeNanos`); `null` when unknown. */
    val fixAgeMs: Long?,
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
    const val MAX_FIX_AGE_MS = 2 * 60 * 1000L
    private const val EARTH_RADIUS_M = 6_371_000.0

    /** The network to offer (SSID cleaned), or `null` when no offer should appear now. */
    fun decide(s: OfferSignals): WifiId? {
        val net = PresenceSignals.usableNetwork(s.wifi) ?: return null
        val cooling = s.dismissedAtMs != null && s.nowMs - s.dismissedAtMs < LATER_COOLDOWN_MS
        val ok = s.saved.isEmpty() && s.playing && !s.showing && !cooling && net.ssid !in s.muted && atHome(s)
        return if (ok) net else null
    }

    /**
     * What the dialog should show now, given the offer on screen ([current]): decided afresh every time (as if nothing were showing),
     * the current offer is kept while its network still qualifies and cleared (without a cooldown) as soon as it does not, or when
     * another network would be offered instead; that one comes on the next check.
     */
    fun next(
        current: WifiId?,
        s: OfferSignals,
    ): WifiId? {
        val fresh = decide(s.copy(showing = false))
        return when {
            current == null -> fresh
            fresh?.ssid == current.ssid -> current
            else -> null
        }
    }

    /** The muted list with [ssid] added as is: it is already cleaned, and cleaning it again would strip a real name's quotes. */
    fun mute(
        muted: Set<String>,
        ssid: String,
    ): Set<String> = muted + ssid

    // A fresh, accurate fix near the pin. The age check keeps out a cached last-known location (it keeps its original time), and a
    // negative age (a fix stamped in the future) never passes.
    private fun atHome(s: OfferSignals): Boolean {
        val fix = s.fix
        val home = s.home
        val fresh = s.fixAgeMs != null && s.fixAgeMs in 0..MAX_FIX_AGE_MS
        return fix != null && home != null && fresh && fix.accuracyM <= MAX_ACCURACY_M && distanceM(fix, home) <= NEAR_HOME_M
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
