package dev.apgo2

private const val APP_FOREGROUND = "app_foreground"
private const val APP_BACKGROUND = "app_background"
private const val MS_PER_MINUTE = 60_000
private const val MINUTES_PER_HOUR = 60
private const val METERS_PER_KM = 1000

/** Plain-text pieces of the "while you were out" report. */
internal object AwayFormat {
    private val LABELS =
        mapOf(
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
            "presence" to "Presence",
            "play_resumed" to "Resumed",
            APP_FOREGROUND to "App opened",
            APP_BACKGROUND to "App left the screen",
        )

    // App foreground/background markers explain gaps in the trace but are not news.
    private val HIDDEN = setOf(APP_FOREGROUND, APP_BACKGROUND)

    fun duration(ms: Long): String {
        val min = ms / MS_PER_MINUTE
        return when {
            min < 1 -> "under a minute"
            min < MINUTES_PER_HOUR -> "$min min"
            min % MINUTES_PER_HOUR == 0L -> "${min / MINUTES_PER_HOUR} h"
            else -> "${min / MINUTES_PER_HOUR} h ${min % MINUTES_PER_HOUR} min"
        }
    }

    fun distance(m: Double): String = if (m >= METERS_PER_KM) "%.1f km".format(m / METERS_PER_KM) else "${m.toInt()} m"

    fun kindLabel(kind: String): String = LABELS[kind] ?: kind

    fun isVisible(kind: String): Boolean = kind !in HIDDEN

    fun visibleKinds(kinds: List<String>): List<String> = kinds.filter(::isVisible)
}

/** Which journal entries the in-app Activity list shows. */
internal object ActivityFormat {
    private val TECHNICAL = setOf("near_miss", "fix_rejected", APP_FOREGROUND, APP_BACKGROUND, "discovered")

    fun shown(
        kind: String,
        details: Boolean,
    ): Boolean = details || kind !in TECHNICAL
}
