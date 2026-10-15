package dev.apgo2.presence

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
    /** The core's nearest camp within reach of a fix (`Engine.nearestCamp`), or `null`. */
    val campNear: (GeoFix) -> OfferCamp?,
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
    const val MAX_ACCURACY_M = 50.0
    const val LATER_COOLDOWN_MS = 10 * 60 * 1000L
    const val MAX_FIX_AGE_MS = 2 * 60 * 1000L

    /** The network to offer (SSID cleaned) and its camp, or `null` when no offer should appear now. */
    fun decide(s: OfferSignals): Offer? {
        val net = PresenceSignals.usableNetwork(s.wifi) ?: return null
        val cooling = s.dismissedAtMs != null && s.nowMs - s.dismissedAtMs < LATER_COOLDOWN_MS
        val camp = nearestCamp(s)
        val needed = camp != null && s.saved.none { it.campId == camp.id } && PresenceSignals.campOf(net, s.saved) == null
        val ok = needed && s.playing && !s.showing && !cooling && net.ssid !in s.muted
        return camp?.takeIf { ok }?.let { Offer(net, it) }
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
     * The camp the player is at: the one the core finds within reach of a fresh, accurate fix. The age check keeps out a cached
     * last-known location (it keeps its original time), and a negative age (a fix stamped in the future) never passes.
     */
    fun nearestCamp(s: OfferSignals): OfferCamp? = usableFix(s.fix, s.fixAgeMs)?.let(s.campNear)

    /** [fix] when it is fresh and accurate enough to say which camp the player is at, else null (see [nearestCamp]). */
    fun usableFix(
        fix: GeoFix?,
        ageMs: Long?,
    ): GeoFix? = fix?.takeIf { ageMs != null && ageMs in 0..MAX_FIX_AGE_MS && it.accuracyM <= MAX_ACCURACY_M }
}
