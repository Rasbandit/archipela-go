package dev.apgo2

import dev.apgo2.ui.Units
import uniffi.apgo_ffi.RecapOut
import java.util.Locale

/** Plain-text pieces of the walk recap. Labels match the "while you were out" report ([AwayFormat]). */
internal object RecapFormat {
    fun stats(r: RecapOut): List<Pair<String, String>> =
        listOf(
            "Time out" to AwayFormat.duration(r.endMs - r.startMs),
            "Distance" to Units.distance(r.distanceM),
            "Steps" to "%,d".format(Locale.US, r.steps),
            "New squares" to r.newCells.toString(),
        )

    fun sections(r: RecapOut): List<Pair<String, List<String>>> =
        listOf(
            AwayFormat.kindLabel("quest_done") to r.questsDone,
            AwayFormat.kindLabel("check_sent") to if (r.checksSent > 0u) listOf(r.checksSent.toString()) else emptyList(),
            AwayFormat.kindLabel("item_received") to r.items,
            AwayFormat.kindLabel("zone_unlocked") to r.zonesUnlocked.map { "Zone $it" },
            "Brought home" to if (r.banked > 0u) listOf("${r.banked} ${if (r.banked == 1u) "item" else "items"}") else emptyList(),
        ).filter { it.second.isNotEmpty() }
}
