package dev.apgo2

import kotlin.math.abs

/**
 * Whether a step reading is worth refreshing the Play screen for: the first one of a game, then only once the count moved by
 * [minSteps] (enough to change what is shown). Standing still costs no refreshes; a reset counter (reboot) counts as a change.
 */
internal class StepRefresh(
    private val minSteps: Long,
) {
    private var last: Long? = null
    private var game: String? = null

    /** [total] is the step counter, [openGame] the open game's id (another game starts over). */
    fun due(
        total: Long,
        openGame: String?,
    ): Boolean {
        val l = last
        if (openGame == game && l != null && abs(total - l) < minSteps) return false
        last = total
        game = openGame
        return true
    }
}
