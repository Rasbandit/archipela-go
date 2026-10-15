package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class OneSaveAtATimeTest {
    private val saves = OneSaveAtATime()
    private val started = mutableListOf<String>()
    private val finish = mutableListOf<() -> Unit>()

    private fun ask(name: String) =
        saves.request { done ->
            started += name
            finish += done
        }

    @Test fun aSaveStartsAtOnceWhenNoneRuns() {
        ask("a")
        assertEquals(listOf("a"), started)
    }

    @Test fun savesAskedForWhileOneRunsWaitAndOnlyTheNewestRuns() {
        ask("a")
        ask("b")
        ask("c")
        assertEquals("nothing starts while a runs", listOf("a"), started)
        finish.removeAt(0)()
        assertEquals(listOf("a", "c"), started)
        finish.removeAt(0)()
        ask("d")
        assertEquals("free again once c is done", listOf("a", "c", "d"), started)
    }
}
