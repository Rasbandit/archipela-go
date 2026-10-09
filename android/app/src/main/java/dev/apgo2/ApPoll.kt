package dev.apgo2

/**
 * How often the Archipelago connection is drained. The client library has no callbacks, so the host must poll it, but only fast
 * right after the server said something: each quiet poll waits longer, up to [SLOW_MS] (an item arriving then shows up a few
 * seconds late, which is nothing for a walking game).
 */
internal class ApPoll {
    private var delayMs = FAST_MS

    /** The wait before the next poll, given whether this one brought any events. */
    fun next(active: Boolean): Long {
        delayMs = if (active) FAST_MS else (delayMs * GROWTH).toLong().coerceAtMost(SLOW_MS)
        return delayMs
    }

    companion object {
        const val FAST_MS = 300L
        const val SLOW_MS = 3_000L
        private const val GROWTH = 1.5

        /** Whether the open game must be synced with the server: items or data changed, or it was never synced. */
        fun needsSync(
            serverChanged: Boolean,
            synced: Boolean,
        ): Boolean = serverChanged || !synced
    }
}
