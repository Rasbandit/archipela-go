package dev.apgo2

import androidx.lifecycle.Lifecycle.State
import org.junit.Assert.assertEquals
import org.junit.Test

class MapLifeTest {
    private val calls = mutableListOf<String>()
    private val life =
        MapLife(
            create = { calls += "create" },
            start = { calls += "start" },
            resume = { calls += "resume" },
            pause = { calls += "pause" },
            stop = { calls += "stop" },
            destroy = { calls += "destroy" },
        )

    private fun taken(): List<String> = calls.toList().also { calls.clear() }

    @Test fun onShowItFollowsTheActivityStepByStep() {
        life.update(State.RESUMED, onShow = true)
        assertEquals(listOf("create", "start", "resume"), taken())
        life.update(State.CREATED, onShow = true)
        assertEquals(listOf("pause", "stop"), taken())
    }

    @Test fun hiddenItStopsButKeepsEverything() {
        life.update(State.RESUMED, onShow = true)
        taken()
        life.update(State.RESUMED, onShow = false)
        assertEquals(listOf("pause", "stop"), taken())
        life.update(State.RESUMED, onShow = true)
        assertEquals(listOf("start", "resume"), taken())
    }

    @Test fun hiddenItIsCreatedButNeverStarted() {
        life.update(State.RESUMED, onShow = false)
        assertEquals(listOf("create"), taken())
    }

    @Test fun theSameStateTwiceDoesNothing() {
        life.update(State.STARTED, onShow = true)
        taken()
        life.update(State.STARTED, onShow = true)
        assertEquals(emptyList<String>(), taken())
    }

    @Test fun destroyWindsDownOnceAndThenIgnoresEverything() {
        life.update(State.RESUMED, onShow = true)
        taken()
        life.destroy()
        assertEquals(listOf("pause", "stop", "destroy"), taken())
        life.destroy()
        life.update(State.RESUMED, onShow = true)
        assertEquals(emptyList<String>(), taken())
    }

    @Test fun destroyBeforeAnythingDoesNothing() {
        life.destroy()
        assertEquals(emptyList<String>(), taken())
    }
}
