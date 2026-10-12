package dev.apgo2

import com.google.android.gms.location.CurrentLocationRequest
import com.google.android.gms.location.Granularity
import com.google.android.gms.location.LocationRequest
import com.google.android.gms.location.Priority

/**
 * How the phone is asked for location: rate, batching and quality from the presence decision and the screen; which provider.
 *
 * The cadence policy (owner decision 2026-10-12, clean trace paths):
 * - In a zone while playing the interval is never slower than 5 s. The map in front (screen on and this app visible): 1 s.
 *   Screen off, or another app in front: 5 s, with hardware batching up to 20 s (every 5 s fix is kept, only delivered in bursts,
 *   so the path stays dense while the CPU sleeps). GNSS quality and no distance filter either way: the core's stationary hold and
 *   gap bridge need a steady stream.
 * - Outside every zone: the coarse rate (90 s, 50 m, balanced). At home or in the car: off.
 * - Leaving a batched request for another one (the screen comes on, the app comes to the front): flush the batch first and change
 *   the request only once the flush completed ([shouldFlush]), so the pin, trace and any quest completion show at once and swapping
 *   the request cannot drop fixes still held in the batch.
 */
internal object GpsPolicy {
    /**
     * [highAccuracy] asks for the GNSS chip (a fused request without it is served as BALANCED: Wi-Fi and cell, 20-100 m off).
     * [maxDelayMs] lets the phone batch fixes (screen off) to save battery.
     */
    data class Rate(
        val intervalMs: Long,
        val minDistanceM: Float,
        val highAccuracy: Boolean = false,
        val maxDelayMs: Long = 0L,
    ) {
        /** How far apart fixes may come (interval plus batching): the core's gap bridge waits this out. */
        val fixSpacingMs: Long get() = intervalMs + maxDelayMs
    }

    /** What a request is built from. [quality] is a `LocationRequest.QUALITY_*` value. */
    data class RequestSpec(
        val intervalMs: Long,
        val minIntervalMs: Long,
        val maxDelayMs: Long,
        val minDistanceM: Float,
        val quality: Int,
    )

    /**
     * The provider to listen to, and one to ask once for a cold-start fix. [GMS_FUSED] means Play services' fused location client
     * for both; otherwise LocationManager provider names (gps, with network once for the cold start).
     */
    data class ProviderPlan(
        val main: String?,
        val coldStart: String?,
    )

    // LocationRequest.QUALITY_HIGH_ACCURACY and QUALITY_BALANCED_POWER_ACCURACY (Android 12), and the provider names, copied so
    // this policy has no API 31 reference.
    private const val QUALITY_HIGH_ACCURACY = 100
    private const val QUALITY_BALANCED = 102
    private const val GPS = "gps"
    private const val NETWORK = "network"
    private const val ON_SCREEN_MS = 1_000L
    private const val OFF_SCREEN_MS = 5_000L

    // Kept well under the core's 45 s gap cap even when a batch arrives late (GpsPolicyTest.screenOffBatchesNeverLookLikeAGpsGap).
    private const val OFF_SCREEN_BATCH_MS = 20_000L
    private const val COLD_START_MAX_AGE_MS = 120_000L

    // Providers that mean location is on (passive is always listed and only relays others' fixes).
    private val LOCATION_SOURCES = setOf("fused", GPS, NETWORK)

    /** Play services' `FusedLocationProviderClient` (Google's fusion), named in the plan and the Diag lines. */
    const val GMS_FUSED = "gms-fused"

    /** Not playing, app on screen: a relaxed rate for the map marker (time-based, plus a distance filter). */
    val IDLE = Rate(intervalMs = 15_000L, minDistanceM = 20f)

    /**
     * In a zone while playing: every second while the map is [onScreen] (screen on and the app in front), else every 5 s batched up
     * to 20 s; GNSS quality and no distance filter in both (see the class comment).
     */
    fun inZone(onScreen: Boolean): Rate =
        if (onScreen) {
            Rate(ON_SCREEN_MS, 0f, highAccuracy = true)
        } else {
            Rate(OFF_SCREEN_MS, 0f, highAccuracy = true, maxDelayMs = OFF_SCREEN_BATCH_MS)
        }

    /**
     * Whether to flush the running request's batch before replacing it with [next]: only a batched request holds fixes back, and only
     * a new request replaces it. Stopping needs none: a stop follows a decision that stops counting, so the core ignores those fixes.
     */
    fun shouldFlush(
        running: Rate?,
        next: Rate?,
    ): Boolean = running != null && next != null && running.maxDelayMs > 0 && running != next

    /** The rate a presence decision asks for; `null` means location is off. Stopped keeps "map marker while the app is on screen". */
    fun forDecision(
        d: dev.apgo2.presence.Decision,
        appVisible: Boolean,
        screenOn: Boolean,
        holding: Boolean = false,
    ): Rate? =
        when {
            // A game is open but presence has not yet learnt whether you are home: no location yet (it would only be stopped again).
            holding -> null

            d.state == dev.apgo2.presence.PresenceState.Stopped -> if (appVisible) IDLE else null

            d.state == dev.apgo2.presence.PresenceState.InZone -> inZone(screenOn && appVisible)

            d.gps is dev.apgo2.presence.GpsMode.Rate -> Rate(d.gps.intervalMs, d.gps.minDistanceM)

            else -> null
        }

    /** The request for [rate]: the minimum interval equals the interval, so the phone never floods the filter. */
    fun request(rate: Rate): RequestSpec =
        RequestSpec(
            rate.intervalMs,
            rate.intervalMs,
            rate.maxDelayMs,
            rate.minDistanceM,
            if (rate.highAccuracy) QUALITY_HIGH_ACCURACY else QUALITY_BALANCED,
        )

    /**
     * The Play services request for [rate]: the same spec as LocationManager's, quality as priority (they share the values). A
     * high-accuracy (in-zone) request waits for an accurate fine fix; the idle marker takes any (Play services' default is to wait).
     */
    fun gmsRequest(rate: Rate): LocationRequest {
        val spec = request(rate)
        val high = spec.quality == QUALITY_HIGH_ACCURACY
        val priority = if (high) Priority.PRIORITY_HIGH_ACCURACY else Priority.PRIORITY_BALANCED_POWER_ACCURACY
        return LocationRequest
            .Builder(priority, spec.intervalMs)
            .setMinUpdateIntervalMillis(spec.minIntervalMs)
            .setMaxUpdateDelayMillis(spec.maxDelayMs)
            .setMinUpdateDistanceMeters(spec.minDistanceM)
            .setWaitForAccurateLocation(high)
            .setGranularity(if (high) Granularity.GRANULARITY_FINE else Granularity.GRANULARITY_PERMISSION_LEVEL)
            .build()
    }

    /** The Play services cold start: one fix from the chip (or one at most 2 minutes old) to start from while updates begin. */
    fun gmsColdStart(): CurrentLocationRequest =
        CurrentLocationRequest
            .Builder()
            .setPriority(Priority.PRIORITY_HIGH_ACCURACY)
            .setMaxUpdateAgeMillis(COLD_START_MAX_AGE_MS)
            .build()

    /**
     * One source, never mixed (network fixes 100 m off made the position jump streets). With Play services: Google's fused location
     * on every Android version, while any location provider is on. Without: gps (our filter does the fusion; AOSP's fused only picks
     * between gps and network), network only if nothing else, and network may give the first fix once.
     */
    fun providers(
        enabled: Set<String>,
        gms: Boolean,
    ): ProviderPlan {
        if (gms) return if (enabled.any { it in LOCATION_SOURCES }) ProviderPlan(GMS_FUSED, GMS_FUSED) else ProviderPlan(null, null)
        val main =
            when {
                GPS in enabled -> GPS
                NETWORK in enabled -> NETWORK
                else -> null
            }
        return ProviderPlan(main, NETWORK.takeIf { main == GPS && it in enabled })
    }
}
