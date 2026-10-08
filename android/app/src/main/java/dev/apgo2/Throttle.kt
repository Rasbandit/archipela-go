package dev.apgo2

/** Lets an action through at most once per [gapMs]. Not thread-safe: call it from one thread (the UI thread). */
internal class Throttle(
    private val gapMs: Long,
) {
    private var last: Long? = null

    fun due(nowMs: Long): Boolean {
        val l = last
        if (l != null && nowMs >= l && nowMs - l < gapMs) return false
        last = nowMs
        return true
    }

    fun reset() {
        last = null
    }
}
