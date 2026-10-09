package dev.apgo2.ui

import uniffi.apgo_ffi.CollectItemOut
import uniffi.apgo_ffi.CollectOut
import uniffi.apgo_ffi.QuestOut

/** The forager items still out there; empty for any other quest. Map pins and tap points use it too. */
internal val QuestOut.openItems: List<CollectItemOut> get() = collect?.items.orEmpty().filterNot { it.picked }

/** Text for a forager quest: its progress row and its item list. Pure, so it is unit-tested. */
internal object CollectFormat {
    fun row(c: CollectOut): String = "carrying ${c.carried} · banked ${c.banked} / ${c.need}"

    fun banked(c: CollectOut): String = "Banked ${c.banked} of ${c.need} ${c.theme}"

    fun item(
        c: CollectOut,
        i: Int,
    ): String = "Item ${i + 1}: ${if (c.items[i].picked) "picked up" else "still out there"}"
}
