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

/** A camp's pin, for choosing which camp a Wi-Fi offer is for. */
internal data class OfferCamp(
    val id: String,
    val name: String,
    val pin: GeoFix,
)

/** A network to offer, with the camp it would be saved for. */
internal data class Offer(
    val net: WifiId,
    val camp: OfferCamp,
)

/** What [HomeWifiOffer.decide] looks at. */
internal data class OfferSignals(
    val saved: List<HomeNetwork>,
    val playing: Boolean,
    val fix: GeoFix?,
    /** How long ago [fix] was taken, on the monotonic clock (`elapsedRealtimeNanos`); `null` when unknown. */
    val fixAgeMs: Long?,
    val camps: List<OfferCamp>,
    val wifi: WifiId?,
    val muted: Set<String>,
    val showing: Boolean,
    val dismissedAtMs: Long?,
    val nowMs: Long,
)

/**
 * "You're at {camp}: add this Wi-Fi?" A player with no Wi-Fi saved for a camp never gets its pause; when a good fix puts them at
 * that camp while on a named network, offer to save that network for it. Pure.
 */
internal object HomeWifiOffer {
    const val NEAR_HOME_M = 75.0
    const val MAX_ACCURACY_M = 50.0
    const val LATER_COOLDOWN_MS = 10 * 60 * 1000L
    const val MAX_FIX_AGE_MS = 2 * 60 * 1000L
    private const val EARTH_RADIUS_M = 6_371_000.0

    /** The network to offer (SSID cleaned) and its camp, or `null` when no offer should appear now. */
    fun decide(s: OfferSignals): Offer? {
        val net = PresenceSignals.usableNetwork(s.wifi) ?: return null
        val cooling = s.dismissedAtMs != null && s.nowMs - s.dismissedAtMs < LATER_COOLDOWN_MS
        val camp = nearestCamp(s)
        val needed = camp != null && s.saved.none { it.campId == camp.id } && PresenceSignals.campOf(net, s.saved) == null
        val ok = needed && s.playing && !s.showing && !cooling && net.ssid !in s.muted
        return camp?.takeIf { ok }?.let { Offer(net, it) }
    }

    /** The camp a Wi-Fi chosen in setup is saved for: the nearest within [NEAR_HOME_M], else the only camp, else none. */
    fun campToSave(
        fix: GeoFix?,
        camps: List<OfferCamp>,
    ): OfferCamp? {
        val near = fix?.let { f -> camps.minByOrNull { distanceM(f, it.pin) }?.takeIf { distanceM(f, it.pin) <= NEAR_HOME_M } }
        return near ?: camps.singleOrNull()
    }

    /**
     * What the dialog should show now, given the offer on screen ([current]): decided afresh every time (as if nothing were showing),
     * the current offer is kept while its network still qualifies and cleared (without a cooldown) as soon as it does not, or when
     * another network would be offered instead; that one comes on the next check.
     */
    fun next(
        current: Offer?,
        s: OfferSignals,
    ): Offer? {
        val fresh = decide(s.copy(showing = false))
        return when {
            current == null -> fresh
            fresh?.net?.ssid == current.net.ssid -> current
            else -> null
        }
    }

    /** The muted list with [ssid] added as is: it is already cleaned, and cleaning it again would strip a real name's quotes. */
    fun mute(
        muted: Set<String>,
        ssid: String,
    ): Set<String> = muted + ssid

    /**
     * The camp the player is at: the nearest within [NEAR_HOME_M] of a fresh, accurate fix. The age check keeps out a cached last-known
     * location (it keeps its original time), and a negative age (a fix stamped in the future) never passes.
     */
    fun nearestCamp(s: OfferSignals): OfferCamp? {
        val fix = s.fix
        val fresh = s.fixAgeMs != null && s.fixAgeMs in 0..MAX_FIX_AGE_MS
        return if (fix == null || !fresh || fix.accuracyM > MAX_ACCURACY_M) {
            null
        } else {
            s.camps.minByOrNull { distanceM(fix, it.pin) }?.takeIf { distanceM(fix, it.pin) <= NEAR_HOME_M }
        }
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
