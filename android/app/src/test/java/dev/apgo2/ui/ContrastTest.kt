package dev.apgo2.ui

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ContrastTest {
    // Every colour drawn on a card or panel, in both themes, against both backgrounds it sits on: text 4.5:1, bars and icons 3:1.
    private val themes = listOf(false to ApgoPalette.lightScheme, true to ApgoPalette.darkScheme)
    private val states = listOf("open", "progress", "done", "locked", "hidden")

    @Test fun blackOnWhiteIsTwentyOneAndSameIsOne() {
        assertEquals(21.0, Contrast.ratio(Color.Black, Color.White), 0.01)
        assertEquals(1.0, Contrast.ratio(ApgoPalette.navy, ApgoPalette.navy), 0.001)
    }

    @Test fun ratioIsSymmetricAndMatchesAKnownPair() {
        // #2E7D32 on #223C61 measured 2.17 in the review that found the unreadable reward text
        assertEquals(2.17, Contrast.ratio(Color(0xFF2E7D32), Color(0xFF223C61)), 0.01)
        assertEquals(Contrast.ratio(Color.Red, Color.Blue), Contrast.ratio(Color.Blue, Color.Red), 1e-9)
    }

    private fun passes(
        need: Double,
        what: String,
        fg: (Boolean) -> Color,
    ) = themes.forEach { (dark, s) ->
        listOf(s.surfaceContainerHighest, s.surface).forEach { bg ->
            val r = Contrast.ratio(fg(dark), bg)
            assertTrue("$what ${if (dark) "dark" else "light"} on ${bg.hex()}: %.2f < $need".format(r), r >= need)
        }
    }

    @Test fun statusTextIsReadableInBothThemes() = Tone.entries.forEach { t -> passes(TEXT, "tone $t") { ApgoPalette.tone(t, it) } }

    @Test fun difficultyIsReadableInBothThemes() =
        listOf("easy", "medium", "hard").forEach { d -> passes(TEXT, "difficulty $d") { ApgoPalette.difficulty(d, it) } }

    @Test fun progressBarsStandOutFromTheCardAndTheirTrack() {
        states.forEach { st -> passes(GRAPHIC, "bar $st") { ApgoPalette.questOnCard(st, it) } }
        themes.forEach { (dark, s) ->
            states.forEach { st ->
                val r = Contrast.ratio(ApgoPalette.questOnCard(st, dark), s.surfaceVariant)
                assertTrue("bar $st on track (dark=$dark): %.2f".format(r), r >= GRAPHIC)
            }
        }
    }

    @Test fun stepNumbersAreReadableOnTheirDot() =
        themes.forEach { (dark, _) ->
            states.forEach { st ->
                val dot = ApgoPalette.questOnCard(st, dark)
                val r = Contrast.ratio(ApgoPalette.inkOn(dot), dot)
                assertTrue("step number on $st dot (dark=$dark): %.2f".format(r), r >= TEXT)
            }
        }

    @Test fun overloadSlotsStandOutAndTheirIconsRead() {
        passes(GRAPHIC, "overload slot") { ApgoPalette.overload(it) }
        themes.forEach { (dark, _) ->
            val fill = ApgoPalette.overload(dark)
            val r = Contrast.ratio(ApgoPalette.inkOn(fill), fill)
            assertTrue("icon on an overload slot (dark=$dark): %.2f".format(r), r >= GRAPHIC)
        }
    }

    @Test fun theBadgeIconStandsOutOnEveryStateColour() =
        states.forEach { st ->
            val r = Contrast.ratio(ApgoPalette.badgeInk(st), ApgoPalette.quest(st))
            assertTrue("badge icon on $st: %.2f".format(r), r >= GRAPHIC)
        }

    private companion object {
        const val TEXT = 4.5
        const val GRAPHIC = 3.0
    }
}
