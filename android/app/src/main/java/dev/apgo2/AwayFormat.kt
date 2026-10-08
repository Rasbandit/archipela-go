package dev.apgo2

/** Plain-text pieces of the "while you were out" report. */
object AwayFormat {
    private val LABELS = mapOf(
        "quest_done" to "Quests completed",
        "check_sent" to "Checks sent",
        "reward" to "Rewards",
        "zone_unlocked" to "Zones unlocked",
        "trap" to "Traps",
        "discovered" to "Places discovered",
        "goal" to "Goals reached",
        "info" to "Notices",
        "item_received" to "Items received",
        "fix_rejected" to "Bad GPS signal",
        "near_miss" to "Near a quest",
        "play_paused" to "Stopped playing",
        "play_resumed" to "Resumed",
        "app_foreground" to "App opened",
        "app_background" to "App left the screen",
    )

    /** App foreground/background markers explain gaps in the trace but are not news. */
    private val HIDDEN = setOf("app_foreground", "app_background")

    fun duration(ms: Long): String {
        val min = ms / 60_000
        return when {
            min < 1 -> "under a minute"
            min < 60 -> "$min min"
            min % 60 == 0L -> "${min / 60} h"
            else -> "${min / 60} h ${min % 60} min"
        }
    }

    fun distance(m: Double): String = if (m >= 1000) "%.1f km".format(m / 1000) else "${m.toInt()} m"

    fun kindLabel(kind: String): String = LABELS[kind] ?: kind

    fun isVisible(kind: String): Boolean = kind !in HIDDEN

    fun visibleKinds(kinds: List<String>): List<String> = kinds.filter(::isVisible)
}

/** Which journal entries the in-app Activity list shows. */
object ActivityFormat {
    private val TECHNICAL = setOf("near_miss", "fix_rejected", "app_foreground", "app_background", "discovered")

    fun shown(kind: String, details: Boolean): Boolean = details || kind !in TECHNICAL
}
