package dev.apgo2

private const val MHZ = 1e6f
private const val TOP_CN0 = 4

/** One satellite of a `GnssStatus` (constellation is a `GnssStatus.CONSTELLATION_*` value; [carrierHz] when the phone reports it). */
internal data class Sat(
    val constellation: Int,
    val used: Boolean,
    val cn0: Float,
    val carrierHz: Float?,
)

private const val OTHER = "other"
private val BANDS =
    listOf(
        "L1" to 1_559f..1_610f,
        "L5" to 1_164f..1_189f,
        "E5b" to 1_189f..1_214f,
        "L2" to 1_215f..1_240f,
        "E6" to 1_260f..1_300f,
    )

/** Which frequency band a carrier frequency (Hz) is in (dual-frequency phones also see L5/E5a). */
internal object GnssBands {
    @Suppress("FunctionNameMinLength") // `of` is the factory name used across these helpers
    fun of(hz: Float): String = BANDS.firstOrNull { (_, mhz) -> hz / MHZ in mhz }?.first ?: OTHER
}

/** The fields of a `gnss` diagnostics line: satellites in view and used per constellation, signal strength and bands. No positions. */
internal object GnssSummary {
    private val names = mapOf(1 to "gps", 2 to "sbas", 3 to "glonass", 4 to "qzss", 5 to "beidou", 6 to "galileo", 7 to "irnss")

    fun fields(sats: List<Sat>): Map<String, Any?> {
        val bands = sats.mapNotNull { s -> s.carrierHz?.let { GnssBands.of(it) } }.filter { it != OTHER }.toSortedSet()
        val byConstellation =
            sats
                .groupBy { names[it.constellation] ?: OTHER }
                .toSortedMap()
                .entries
                .joinToString(",") { (name, list) -> "$name=${list.count { it.used }}/${list.size}" }
        val top = sats.map { it.cn0.toDouble() }.sortedDescending().take(TOP_CN0)
        return linkedMapOf(
            "in_view" to sats.size,
            "used" to sats.count { it.used },
            "by_constellation" to byConstellation,
            "cn0_top4" to if (top.isEmpty()) 0.0 else top.average(),
            "bands" to bands.joinToString(","),
            "dual_freq" to (bands.size > 1),
        )
    }
}

// A GnssStatus older than this says nothing about the fix in hand (ms).
private const val GNSS_EVIDENCE_MS = 30_000L
private const val NETWORK_SOURCE = "network"

/**
 * Whether Play services' fused fixes come from satellites (adversarial review I2). With the GPS toggle off, fused returns Wi-Fi and
 * cell positions of 10 to 30 m tagged `fused`; those must reach the core as `network` (display only, never counted for quests). A
 * fused fix is network when a GnssStatus reported within the last 30 s with no satellite used in a fix. With no GnssStatus data (it
 * runs only in a zone, or the phone gives none), fused is trusted. Times are `elapsedRealtime` ms.
 */
internal class GnssEvidence {
    @Volatile private var lastStatusMs: Long? = null

    @Volatile private var used = 0

    /** A GnssStatus arrived at [elapsedMs] with [used] satellites used in a fix. */
    fun onStatus(
        used: Int,
        elapsedMs: Long,
    ) {
        this.used = used
        lastStatusMs = elapsedMs
    }

    /** Fresh GnssStatus data says no satellite is used in a fix. */
    fun networkOnly(elapsedMs: Long): Boolean = lastStatusMs?.let { elapsedMs - it <= GNSS_EVIDENCE_MS && used == 0 } ?: false

    /** The provider to send to the core for a fix tagged [provider]: a fused fix without satellites is [NETWORK_SOURCE]. */
    fun providerOf(
        provider: String,
        elapsedMs: Long,
    ): String = if (provider == FUSED_SOURCE && networkOnly(elapsedMs)) NETWORK_SOURCE else provider
}

/** Nearest-rank percentiles of a minute's `onFix` times (heartbeat perf fields). */
internal object Percentiles {
    @Suppress("FunctionNameMinLength") // `of` mirrors GnssBands.of
    fun of(
        values: List<Long>,
        q: Double,
    ): Long {
        if (values.isEmpty()) return 0L
        val sorted = values.sorted()
        val rank =
            kotlin.math
                .ceil(q * sorted.size)
                .toInt()
                .coerceIn(1, sorted.size)
        return sorted[rank - 1]
    }
}

/** The newest [cap] samples (older ones are dropped), so a long gap between heartbeats cannot grow without bound. */
internal class RecentSamples(
    private val cap: Int,
) {
    private val items = ArrayDeque<Long>()
    val values: List<Long> get() = items.toList()

    fun add(v: Long) {
        if (items.size >= cap) items.removeFirst()
        items.addLast(v)
    }

    fun clear() = items.clear()
}
