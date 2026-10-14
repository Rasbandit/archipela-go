package dev.apgo2

import dev.apgo2.ui.Help
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RecapNoticeTest {
    // On screen the recap opens at once; off screen it waits behind a notification.
    @Test fun notifiesOnlyOffScreen() {
        assertTrue(RecapNotice.shouldPost(appVisible = false))
        assertFalse(RecapNotice.shouldPost(appVisible = true))
    }

    @Test fun textSaysHowLongAndHowManyQuests() {
        val r = recap(minutes = 42, quests = listOf("A", "B"))
        assertEquals("42 min out, 2 quests done", Help.recapNoticeText(r))
        assertEquals("42 min out", Help.recapNoticeText(recap(minutes = 42, quests = emptyList())))
        assertEquals("42 min out, 1 quest done", Help.recapNoticeText(recap(minutes = 42, quests = listOf("A"))))
    }
}

internal fun recap(
    minutes: Long,
    quests: List<String> = emptyList(),
) = uniffi.apgo_ffi.RecapOut(
    startMs = 0,
    endMs = minutes * 60_000,
    steps = 0,
    distanceM = 0.0,
    newCells = 0u,
    questsDone = quests,
    countedSteps = 0,
    chains = emptyList(),
    quests = emptyList(),
    checksSent = 0u,
    items = emptyList(),
    zonesUnlocked = emptyList(),
    banked = 0u,
    route = emptyList(),
)

class RecapDismissTest {
    // A previewed recap is sample data: dismissing it must never reach the core.
    @Test fun dismissTouchesCoreOnlyForRealRecaps() {
        assertTrue(RecapController.dismissTouchesCore(previewing = false))
        assertFalse(RecapController.dismissTouchesCore(previewing = true))
    }
}
