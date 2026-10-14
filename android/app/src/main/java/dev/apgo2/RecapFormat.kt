package dev.apgo2

import dev.apgo2.ui.Units
import uniffi.apgo_ffi.RecapChainOut
import uniffi.apgo_ffi.RecapOut
import uniffi.apgo_ffi.RecapQuestOut
import java.text.DateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.roundToLong

/** Plain-text pieces of the walk recap. Labels match the "while you were out" report ([AwayFormat]). */
internal object RecapFormat {
    /** "[date] · 3.1 km · 42 min" under the title. */
    fun subtitle(r: RecapOut): String {
        val date = DateFormat.getDateInstance(DateFormat.SHORT).format(Date(r.endMs))
        return "$date · ${Units.distance(r.distanceM)} · ${AwayFormat.duration(r.endMs - r.startMs)}"
    }

    /** What a chain's bar gained so far, at counter [value]. */
    fun barGain(
        c: RecapChainOut,
        value: Double,
    ): String {
        val gain = (value - c.before).roundToLong()
        return when (c.unit) {
            "minutes" -> "+$gain min"
            "cells" -> "+$gain new squares"
            else -> "+${count(gain)} steps"
        }
    }

    /** "6,230 total (was 2,140)" for a chain's bar at counter [value]. */
    fun barTotal(
        c: RecapChainOut,
        value: Double,
    ): String = "${count(value.roundToLong())} total (was ${count(c.before.roundToLong())})"

    /** The next milestone past [value], or that they are all done. */
    fun barNext(
        c: RecapChainOut,
        value: Double,
    ): String = c.marks.firstOrNull { it > value }?.let { "next: ${count(it.roundToLong())}" } ?: "All milestones reached"

    /** "4,812 walked · 4,090 counted", or null when every step counted. */
    fun stepsLine(r: RecapOut): String? =
        if (r.steps == r.countedSteps) null else "${count(r.steps)} walked · ${count(r.countedSteps)} counted"

    /** The short time of day a quest was finished. */
    fun questTime(q: RecapQuestOut): String = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(q.tMs))

    private fun count(n: Long) = "%,d".format(Locale.US, n)

    fun sections(r: RecapOut): List<Pair<String, List<String>>> =
        listOf(
            AwayFormat.kindLabel("check_sent") to if (r.checksSent > 0u) listOf(r.checksSent.toString()) else emptyList(),
            AwayFormat.kindLabel("item_received") to r.items,
            AwayFormat.kindLabel("zone_unlocked") to r.zonesUnlocked.map { "Zone $it" },
            "Brought home" to if (r.banked > 0u) listOf("${r.banked} ${if (r.banked == 1u) "item" else "items"}") else emptyList(),
        ).filter { it.second.isNotEmpty() }
}
