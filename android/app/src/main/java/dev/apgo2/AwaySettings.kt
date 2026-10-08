package dev.apgo2

/** The "time away" distance chosen in New Game. 0 asks the core to pick one from the realm's size. */
object AwaySettings {
    private const val DEFAULT_M = 1000u
    private const val MAX_M = 20_000u

    fun distance(auto: Boolean, text: String): UInt {
        if (auto) return 0u
        val n = text.trim().toULongOrNull() ?: return DEFAULT_M
        return if (n == 0uL) DEFAULT_M else minOf(n, MAX_M.toULong()).toUInt()
    }
}
