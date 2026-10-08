package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/**
 * Holds the one live session. A replaced session is closed at once, or, if a [use] (a poll) is still running on it, when
 * that use returns: closing it mid-poll would make the poll's next call hit a destroyed native object. Main thread only.
 */
internal class SessionSlot<S : Any>(
    private val close: (S) -> Unit,
) {
    var current by mutableStateOf<S?>(null)
        private set
    private var inUse: S? = null

    fun replace(next: S?) {
        val old = current
        current = next
        if (old != null && old !== next && old !== inUse) close(old)
    }

    /** Run [block] on the current session; inline so a suspending poll can run inside it. */
    inline fun <R> use(block: (S) -> R): R? {
        val s = current ?: return null
        begin(s)
        try {
            return block(s)
        } finally {
            end(s)
        }
    }

    @PublishedApi internal fun begin(s: S) {
        inUse = s
    }

    @PublishedApi internal fun end(s: S) {
        inUse = null
        if (current !== s) close(s)
    }
}
