package dev.apgo2

import dev.apgo2.ui.Units
import uniffi.apgo_ffi.RecapChainOut
import uniffi.apgo_ffi.RecapForagerOut
import uniffi.apgo_ffi.RecapOut
import uniffi.apgo_ffi.RecapQuestOut
import java.text.DateFormat
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.roundToLong

/** Plain-text pieces of the walk recap. Labels match the "while you were out" report ([AwayFormat]). */
internal object RecapFormat {
    /** "Tue 10/14 · 5:12 PM – 5:54 PM · 3.1 km · 42 min" under the title. */
    fun subtitle(
        r: RecapOut,
        time: DateFormat = DateFormat.getTimeInstance(DateFormat.SHORT),
        day: DateFormat = SimpleDateFormat("EEE M/d", Locale.getDefault()),
    ): String {
        val dayText = day.format(Date(r.startMs))
        val span = "${time.format(Date(r.startMs))} \u2013 ${time.format(Date(r.endMs))}"
        val length = AwayFormat.duration(r.endMs - r.startMs)
        return "$dayText \u00b7 $span \u00b7 ${Units.distance(r.distanceM)} \u00b7 $length"
    }

    /** "Pinecones · +2 banked · 5 / 7" for a forager's bar at the animated banked counter [banked]. */
    fun foragerLine(
        f: RecapForagerOut,
        banked: Double,
    ): String =
        "${f.theme.replaceFirstChar {
            it.uppercase()
        }} \u00b7 +${banked.roundToLong() - f.bankedBefore.toLong()} banked \u00b7 ${banked.roundToLong()} / ${f.need}"

    /** "2 still in your bag", or null when nothing is left unbanked. */
    fun bagLine(f: RecapForagerOut): String? = if (f.carried == 0u) null else "${f.carried} still in your bag"

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
        ).filter { it.second.isNotEmpty() }
}
