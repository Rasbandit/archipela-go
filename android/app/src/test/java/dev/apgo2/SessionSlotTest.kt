package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

class SessionSlotTest {
    private class Fake(
        val name: String,
    )

    private val closed = mutableListOf<String>()
    private val slot = SessionSlot<Fake> { closed += it.name }

    @Test fun replacingAnIdleSessionClosesItAtOnce() {
        slot.replace(Fake("a"))
        slot.replace(Fake("b"))
        assertEquals(listOf("a"), closed)
        assertEquals("b", slot.current?.name)
    }

    @Test fun replacingASessionInUseClosesItWhenTheUseEnds() {
        slot.replace(Fake("a"))
        slot.use { a ->
            slot.replace(Fake("b"))
            assertEquals(emptyList<String>(), closed) // a is still being polled
            a.name
        }
        assertEquals(listOf("a"), closed)
        assertEquals("b", slot.current?.name)
    }

    @Test fun aReconnectLeavesExactlyOneLiveSession() {
        val made = mutableListOf<String>()

        fun connect(n: String) = slot.replace(Fake(n).also { made += it.name })
        connect("a")
        slot.use {
            connect("b")
            connect("c")
        }
        assertEquals(listOf("b", "a"), closed) // b was idle, a waited for its poll
        assertEquals(made - closed.toSet(), listOf(slot.current?.name))
    }

    @Test fun theSessionInUseIsNotClosedWhenNothingReplacesIt() {
        slot.replace(Fake("a"))
        assertEquals("a", slot.use { it.name })
        assertEquals(emptyList<String>(), closed)
    }

    @Test fun aFailingUseStillClosesTheReplacedSession() {
        slot.replace(Fake("a"))
        runCatching {
            slot.use {
                slot.replace(Fake("b"))
                error("cancelled mid-poll")
            }
        }
        assertEquals(listOf("a"), closed)
    }

    @Test fun clearingClosesTheSession() {
        slot.replace(Fake("a"))
        slot.replace(null)
        assertEquals(listOf("a"), closed)
        assertNull(slot.current)
    }

    @Test fun useWithoutASessionDoesNothing() {
        assertNull(slot.use { it.name })
    }

    @Test fun replacingWithTheSameSessionKeepsItOpen() {
        val a = Fake("a")
        slot.replace(a)
        slot.replace(a)
        assertEquals(emptyList<String>(), closed)
        assertSame(a, slot.current)
    }
}
