package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.apgo_ffi.CollectItemOut
import uniffi.apgo_ffi.CollectOut
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.QuestOut

class CollectFormatTest {
    private fun collect(
        carried: Int,
        banked: Int,
        need: Int,
        vararg picked: Boolean,
    ) = CollectOut(
        theme = "pinecones",
        need = need.toUInt(),
        carried = carried.toUInt(),
        banked = banked.toUInt(),
        items = picked.mapIndexed { i, p -> CollectItemOut(at = GeoPoint(lat = i.toDouble(), lon = 0.0), picked = p) },
    )

    private fun quest(collect: CollectOut?) =
        QuestOut(
            locationId = 1,
            zone = 1u,
            name = "q",
            place = "",
            family = "courier",
            kindId = "forager",
            difficulty = "Easy",
            tier = 1u,
            effortMin = 10.0,
            mode = "walk",
            state = "open",
            progress = 0f,
            shape = "collect",
            anchor = null,
            anchorB = null,
            radiusM = 0.0,
            path = emptyList(),
            detail = "",
            fallback = false,
            boss = false,
            blurb = "",
            reward = null,
            chainId = null,
            collect = collect,
        )

    @Test fun theRowSaysWhatIsCarriedAndBanked() {
        assertEquals("carrying 2 · banked 3 / 5", CollectFormat.row(collect(2, 3, 5)))
        assertEquals("carrying 0 · banked 0 / 3", CollectFormat.row(collect(0, 0, 3)))
        assertEquals("carrying 0 · banked 6 / 5", CollectFormat.row(collect(0, 6, 5)))
    }

    @Test fun theDetailsNameTheThemeAndEveryItem() {
        val c = collect(1, 3, 5, true, false)
        assertEquals("Banked 3 of 5 pinecones", CollectFormat.banked(c))
        assertEquals("Item 1: picked up", CollectFormat.item(c, 0))
        assertEquals("Item 2: still out there", CollectFormat.item(c, 1))
    }

    @Test fun openItemsAreTheUnpickedOnes() {
        val open = quest(collect(1, 0, 3, true, false, false)).openItems
        assertEquals(listOf(1.0, 2.0), open.map { it.at.lat })
    }

    @Test fun aQuestWithoutItemsOrWithAllPickedHasNoOpenItems() {
        assertEquals(emptyList<CollectItemOut>(), quest(null).openItems)
        assertEquals(emptyList<CollectItemOut>(), quest(collect(2, 0, 2, true, true)).openItems)
    }
}
