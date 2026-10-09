package dev.apgo2.ui

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PaletteTest {
    @Test fun hexIsLowercaseRgbWithoutAlpha() {
        assertEquals("#11233e", ApgoPalette.navy.hex())
        assertEquals("#ffffff", Color.White.hex())
        assertEquals("#000000", Color.Black.hex())
        assertEquals("alpha is dropped", "#ff0000", Color(0x80FF0000).hex())
        assertEquals("leading zeros are kept", "#00acc1", ApgoPalette.thaw.hex())
    }

    @Test fun eachQuestStateHasItsColourAndUnknownReadsAsTodo() {
        assertEquals(ApgoPalette.questDone, ApgoPalette.quest("done"))
        assertEquals(ApgoPalette.questProgress, ApgoPalette.quest("progress"))
        assertEquals(ApgoPalette.questLocked, ApgoPalette.quest("locked"))
        assertEquals(ApgoPalette.questHidden, ApgoPalette.quest("hidden"))
        assertEquals(ApgoPalette.questTodo, ApgoPalette.quest("open"))
        assertEquals(ApgoPalette.questTodo, ApgoPalette.quest(""))
    }

    @Test fun questStateColoursAreDistinct() {
        val states = listOf("done", "progress", "locked", "hidden", "open")
        assertEquals(states.size, states.map { ApgoPalette.quest(it) }.toSet().size)
    }

    @Test fun notDoneDoesNotLookLikeAnError() {
        val todo = ApgoPalette.quest("open")
        assertNotEquals(ApgoPalette.danger, todo)
        assertNotEquals(ApgoPalette.banned, todo)
        assertTrue("not done reads as calm, not red", todo.blue > todo.red)
    }

    @Test fun doableIsBlueAndYourTrailAndTheRealmBlendIn() {
        val todo = ApgoPalette.questTodo
        assertTrue("doable is blue", todo.blue > todo.red && todo.blue > todo.green)
        val realm = ApgoPalette.realm
        assertTrue(
            "the realm boundary is a neutral grey",
            maxOf(realm.red, realm.green, realm.blue) - minOf(realm.red, realm.green, realm.blue) < 0.12f,
        )
    }

    @Test fun yourTrailIsAPaleColourOfItsOwn() {
        val trace = ApgoPalette.trace
        assertTrue("pale: every channel light", minOf(trace.red, trace.green, trace.blue) > 0.55f)
        listOf("open", "progress", "done", "locked").forEach { assertNotEquals("never a quest state ($it)", ApgoPalette.quest(it), trace) }
    }

    @Test fun notDoneStandsApartFromTheRealmAndTrace() {
        assertNotEquals(ApgoPalette.realm, ApgoPalette.questTodo)
        assertNotEquals(ApgoPalette.trace, ApgoPalette.questTodo)
    }

    @Test fun everyFamilyHasItsOwnColourAndUnknownFallsBackToTeal() {
        val families = listOf("reach", "dwell", "landmark", "trail", "park", "water", "courier", "explore", "steps", "away", "boss")
        assertEquals(families.size, families.map { ApgoPalette.family(it) }.toSet().size)
        assertEquals(ApgoPalette.teal, ApgoPalette.family("no_such_family"))
        assertEquals(ApgoPalette.teal, ApgoPalette.family(""))
    }

    @Test fun landmarkSubGroupsOverrideTheFamilyColour() {
        val landmark = ApgoPalette.family("landmark")
        val culture = ApgoPalette.kind("mural_mural", "landmark")
        val food = ApgoPalette.kind("caffeine_quest", "landmark")
        assertNotEquals(landmark, culture)
        assertNotEquals(culture, food)
        assertEquals("same group, same colour", culture, ApgoPalette.kind("museum_mile", "landmark"))
    }

    @Test fun aKindWithoutAGroupUsesItsFamily() {
        assertEquals(ApgoPalette.family("dwell"), ApgoPalette.kind("bench_warmer", "dwell"))
        assertEquals(ApgoPalette.teal, ApgoPalette.kind("unknown", "unknown"))
    }

    @Test fun theUncertainPinColourDiffersFromTheNormalOne() {
        assertTrue(ApgoPalette.me != ApgoPalette.meUncertain)
    }
}
