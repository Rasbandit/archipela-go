package dev.apgo2.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb

private const val RGB_MASK = 0xFFFFFF

/** "#rrggbb" for MapLibre style expressions, which take strings rather than Compose colours. */
internal fun Color.hex(): String = "#%06x".format(toArgb() and RGB_MASK)

/**
 * Every colour the app uses lives here, so Compose screens and the map draw from one source.
 * Brand colours come from Archipelago's web theme (ArchipelagoMW/Archipelago, WebHostLib/static/styles, MIT);
 * only colours are used, the Archipelago logo is CC BY-NC 4.0 and is deliberately not bundled.
 */
internal object ApgoPalette {
    // Archipelago ocean theme (#11233e panels, #93dcff headings, #fffc95 links) and header teals
    val navy = Color(0xFF11233E)
    val teal = Color(0xFF2F6B83)
    val sky = Color(0xFF93DCFF)
    val mint = Color(0xFFD0EBE6)
    val butter = Color(0xFFFFFC95)
    val grass = Color(0xFF5AFF6A) // Archipelago grass theme

    // Quest state: the list dot and the map marker must agree
    val questTodo = Color(0xFFD32F2F)
    val questProgress = Color(0xFFF9A825)
    val questDone = Color(0xFF2E7D32)
    val questLocked = Color(0xFF9E9E9E)
    val questHidden = Color(0xFFBDBDBD)

    // Map and realm editor
    val me = Color(0xFF1565C0)
    val realm = Color(0xFF1565C0)
    val home = Color(0xFF2E7D32)
    val draft = Color(0xFFEF6C00) // the shape being edited
    val draftStrong = Color(0xFFBF360C) // radius line and label
    val thaw = Color(0xFF00ACC1)
    val waypoint = Color(0xFF8E24AA)
    val onMap = Color.White // halos, outlines and knob fills

    // One colour per quest family: pins and icons of a kind of find share it. All hold white text/glyphs.
    private val families =
        mapOf(
            "reach" to Color(0xFF2F6B83),
            "dwell" to Color(0xFFC77700),
            "landmark" to Color(0xFF455A64),
            "trail" to Color(0xFF3A8F3A),
            "park" to Color(0xFF689F38),
            "water" to Color(0xFF1976D2),
            "courier" to Color(0xFFB3472E),
            "explore" to Color(0xFF3F51B5),
            "steps" to Color(0xFF546E7A),
            "away" to Color(0xFFAD1457),
            "boss" to Color(0xFF8D6E00),
        )

    // The landmark family is half the catalog, so it is split into sub-groups with a colour each.
    private val landmarkGroups: Map<Color, List<String>> =
        mapOf(
            // culture
            Color(0xFF7B4FB5) to
                listOf(
                    "gallery_walls",
                    "mural_mural",
                    "remember_when",
                    "time_traveler",
                    "museum_mile",
                    "steeple_chase",
                    "quiet_please",
                    "campus_crawl",
                ),
            // food and drink
            Color(0xFFF57C00) to
                listOf(
                    "brain_freeze",
                    "rise_and_shine",
                    "caffeine_quest",
                    "market_day",
                    "grape_expectations",
                    "hydration_station",
                ),
            Color(0xFF5D4037) to listOf("bus_stop_bingo", "all_aboard", "rack_em_up", "plug_in"), // getting around
            // sport and play
            Color(0xFF0097A7) to
                listOf(
                    "nothing_but_net",
                    "love_all",
                    "dill_with_it",
                    "court_jester",
                    "game_day",
                    "outdoor_gym",
                    "thrill_seeker",
                    "merry_go_round",
                    "lost_in_the_maze",
                ),
            Color(0xFF00796B) to
                listOf(
                    "eagle_eye",
                    "summit_fever",
                    "chasing_waterfalls",
                    "dam_good_walk",
                    "sand_between_toes",
                    "lighthouse_keeper",
                    "wild_side",
                    "spring_fling",
                    "tree_hugger",
                    "spelunker",
                    "hot_stuff",
                    "make_a_wish",
                    "dock_of_the_bay",
                    "bridge_troll",
                ),
            // views and nature
            // city services
            Color(0xFF455A64) to
                listOf(
                    "hydrant_hunter",
                    "mailbox_maven",
                    "flag_day",
                    "tower_defense",
                    "pit_stop",
                    "know_your_block",
                    "camp_out",
                    "hut_to_hut",
                ),
        )
    private val kindColors: Map<String, Color> = landmarkGroups.flatMap { (color, ids) -> ids.map { it to color } }.toMap()

    // Marks on places
    val favorite = Color(0xFFF9A825)
    val banned = Color(0xFFC62828)

    // Feedback text
    val warning = Color(0xFFE65100)
    val danger = Color(0xFFC62828)
    val success = Color(0xFF2E7D32)
    val muted = Color(0xFF757575)

    fun quest(state: String): Color =
        when (state) {
            "done" -> questDone
            "progress" -> questProgress
            "locked" -> questLocked
            "hidden" -> questHidden
            else -> questTodo
        }

    fun family(family: String): Color = families[family] ?: teal

    /** The colour of a find or quest of this kind: its landmark sub-group, else its family. */
    fun kind(
        kindId: String,
        family: String,
    ): Color = kindColors[kindId] ?: family(family)
}
