package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.apgo_ffi.GeoPoint

class RecapSampleTest {
    private val now = 1_700_000_000_000L
    private val sample = RecapSample.build(GeoPoint(40.7608, -111.8910), now)

    @Test fun everyChainCrossesAMilestone() {
        assertEquals(3, sample.chains.size)
        sample.chains.forEach { c -> assertTrue(c.name, c.marks.any { it > c.before && it <= c.after }) }
    }

    @Test fun countedStepsDifferFromSteps() = assertNotEquals(sample.steps, sample.countedSteps)

    @Test fun threeQuestsOnTheRouteOneWithoutReward() {
        assertEquals(3, sample.quests.size)
        val route = sample.route.flatten()
        sample.quests.forEach { q -> assertTrue(q.name, q.at in route) }
        assertEquals(1, sample.quests.count { it.reward == null })
        assertNull(sample.quests.single { it.name == "Trail sampler" }.reward)
    }

    @Test fun routeIsAClosedLoop() {
        val loop = sample.route.single()
        assertTrue(loop.size >= 8)
        assertEquals(loop.first(), loop.last())
    }

    @Test fun walkLastedFortyTwoMinutes() {
        assertEquals(42 * 60_000L, sample.endMs - sample.startMs)
        assertEquals(now, sample.endMs)
    }

    @Test fun twoForagersOneCompletingItsMark() {
        val (pine, leaves) = sample.foragers
        assertEquals(listOf(7u, 5u, 7u, 2u), listOf(pine.need, pine.bankedBefore, pine.bankedAfter, pine.picked))
        assertEquals(listOf(7u, 3u, 4u, 1u), listOf(leaves.need, leaves.bankedBefore, leaves.bankedAfter, leaves.picked))
        assertTrue(pine.bankedBefore < pine.need && pine.bankedAfter >= pine.need)
        assertTrue(leaves.bankedAfter < leaves.need)
    }
}
