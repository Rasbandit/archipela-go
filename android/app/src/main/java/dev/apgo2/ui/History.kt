package dev.apgo2.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue

/**
 * An undo/redo history of editor states. [push] records a new state after an edit (and forgets anything that could be redone),
 * [undo] and [redo] step along it. [canUndo] and [canRedo] are observable, so buttons enable and disable themselves.
 */
class History<T>(initial: T, private val limit: Int = 100) {
    private val past = ArrayDeque<T>()
    private val future = ArrayDeque<T>()
    private var version by mutableIntStateOf(0)

    var current: T = initial
        private set

    val canUndo: Boolean get() = version >= 0 && past.isNotEmpty()
    val canRedo: Boolean get() = version >= 0 && future.isNotEmpty()

    /** Record [state] as the newest one. Equal to the current state: nothing happens. */
    fun push(state: T) {
        if (state == current) return
        past.addLast(current)
        if (past.size > limit) past.removeFirst()
        current = state
        future.clear()
        version++
    }

    /** Step back; returns the state to show, or null at the start. */
    fun undo(): T? {
        val previous = past.removeLastOrNull() ?: return null
        future.addLast(current)
        current = previous
        version++
        return previous
    }

    /** Step forward; returns the state to show, or null at the newest. */
    fun redo(): T? {
        val next = future.removeLastOrNull() ?: return null
        past.addLast(current)
        current = next
        version++
        return next
    }

    /** Make [state] the new starting point: nothing before it can be undone any more. */
    fun reset(state: T) {
        past.clear()
        future.clear()
        current = state
        version++
    }
}
