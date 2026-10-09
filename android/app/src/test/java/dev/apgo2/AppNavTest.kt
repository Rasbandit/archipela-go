package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppNavTest {
    @Test fun itStartsOnPlayWithNewGameClosed() {
        val nav = AppNav()
        assertEquals(AppTab.PLAY, nav.tab)
        assertFalse(nav.newGameOpen)
        assertFalse("nothing to go back to", nav.canGoBack)
    }

    @Test fun newGameOpensOverPlay() {
        val nav = AppNav()
        nav.show(AppTab.SETTINGS)
        nav.openNewGame()
        assertEquals("Play stays lit in the bar", AppTab.PLAY, nav.tab)
        assertTrue(nav.newGameOpen)
    }

    @Test fun anyTabClosesNewGame() {
        listOf(AppTab.PLAY, AppTab.REALMS, AppTab.ACTIVITY, AppTab.SETTINGS).forEach { t ->
            val nav = AppNav()
            nav.openNewGame()
            nav.show(t)
            assertEquals(t, nav.tab)
            assertFalse("tab $t closes New Game", nav.newGameOpen)
        }
    }

    @Test fun backClosesNewGameToTheGamesList() {
        val nav = AppNav()
        nav.openNewGame()
        assertTrue(nav.canGoBack)
        nav.back()
        assertEquals(AppTab.PLAY, nav.tab)
        assertFalse(nav.newGameOpen)
        assertFalse(nav.canGoBack)
    }

    @Test fun backFromAnotherTabGoesToPlay() {
        val nav = AppNav()
        nav.show(AppTab.ACTIVITY)
        assertTrue(nav.canGoBack)
        nav.back()
        assertEquals(AppTab.PLAY, nav.tab)
    }

    @Test fun backOnPlayChangesNothing() {
        val nav = AppNav()
        nav.back()
        assertEquals(AppTab.PLAY, nav.tab)
        assertFalse(nav.newGameOpen)
    }

    @Test fun theBarHasNoNewGameTab() {
        assertEquals(listOf(0, 1, 2, 3), listOf(AppTab.PLAY, AppTab.REALMS, AppTab.ACTIVITY, AppTab.SETTINGS))
    }
}
