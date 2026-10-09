package dev.apgo2

import kotlin.math.abs

/**
 * Whether a step reading is worth refreshing the Play screen for: the first one, then only once the count moved by [minSteps]
 * (enough to change what is shown). Standing still costs no refreshes; a reset counter (reboot) counts as a change.
 */
internal class StepRefresh(
    private val minSteps: Long,
) {
    private var last: Long? = null

    fun due(total: Long): Boolean {
        val l = last
        if (l != null && abs(total - l) < minSteps) return false
        last = total
        return true
    }
}
