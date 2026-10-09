package dev.apgo2

import androidx.lifecycle.Lifecycle.State

/**
 * Drives a map view's own lifecycle calls. It follows the activity, but only runs (started, drawing) while the map is on show: a
 * hidden map is stopped, keeping its style, tiles and camera for an instant return. [destroy] winds it down for good.
 */
internal class MapLife(
    private val create: () -> Unit,
    private val start: () -> Unit,
    private val resume: () -> Unit,
    private val pause: () -> Unit,
    private val stop: () -> Unit,
    private val destroy: () -> Unit,
) {
    private var at = State.INITIALIZED

    /** Step to where the activity is ([activity]), held at created while the map is not [onShow]. */
    fun update(
        activity: State,
        onShow: Boolean,
    ) {
        if (at == State.DESTROYED) return
        val target = minOf(activity, if (onShow) State.RESUMED else State.CREATED).coerceAtLeast(State.CREATED)
        while (at < target) stepUp()
        while (at > target) stepDown()
    }

    /** Wind down for good (the map left the screen or the activity ended). */
    fun destroy() {
        if (at == State.INITIALIZED || at == State.DESTROYED) return
        while (at > State.CREATED) stepDown()
        destroy.invoke()
        at = State.DESTROYED
    }

    private fun stepUp() {
        when (at) {
            State.INITIALIZED -> create()
            State.CREATED -> start()
            else -> resume()
        }
        at = State.entries[at.ordinal + 1]
    }

    private fun stepDown() {
        if (at == State.RESUMED) pause() else stop()
        at = State.entries[at.ordinal - 1]
    }
}
