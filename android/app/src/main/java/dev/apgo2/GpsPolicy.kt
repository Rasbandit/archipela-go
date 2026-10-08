package dev.apgo2

/** How often the phone is asked for a location fix. Finer while a game is running, relaxed otherwise (battery). */
object GpsPolicy {
    data class Rate(val intervalMs: Long, val minDistanceM: Float)

    private val PLAYING = Rate(intervalMs = 5_000L, minDistanceM = 0f) // time-based only: standing still must still produce fixes (Dwell, Away)
    private val IDLE = Rate(intervalMs = 15_000L, minDistanceM = 20f)

    fun forState(playing: Boolean): Rate = if (playing) PLAYING else IDLE

    /**
     * The one provider to listen to. Mixing providers interleaves fixes of very different quality (network fixes can be hundreds of
     * metres off), which showed up as the position jumping between streets: take fused (Android 12+), else GPS, network only if nothing else.
     */
    fun providers(enabled: Set<String>, sdk: Int): List<String> =
        listOf("fused".takeIf { sdk >= 31 }, "gps", "network").filterNotNull().firstOrNull { it in enabled }?.let { listOf(it) } ?: emptyList()
}
