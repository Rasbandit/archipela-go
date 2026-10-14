package dev.apgo2.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb

private const val RGB_MASK = 0xFFFFFF

/** "#rrggbb" for MapLibre style expressions, which take strings rather than Compose colours. */
internal fun Color.hex(): String = "#%06x".format(toArgb() and RGB_MASK)

/**
 * Every colour the app uses lives here, so Compose screens and the map draw from one source; `scripts/check_color_tokens.sh`
 * fails the build on colour literals anywhere else.
 * Brand colours come from Archipelago's web theme (ArchipelagoMW/Archipelago, WebHostLib/static/styles, MIT);
 * only colours are used, the Archipelago logo is CC BY-NC 4.0 and is deliberately not bundled.
 */
internal object ApgoPalette {
    private val white = Color.White

    // Archipelago ocean theme (#11233e panels, #93dcff headings, #fffc95 links) and header teals
    val navy = Color(0xFF11233E)
    val teal = Color(0xFF2F6B83)
    val sky = Color(0xFF93DCFF)
    val mint = Color(0xFFD0EBE6)
    val butter = Color(0xFFFFFC95)
    val grass = Color(0xFF5AFF6A) // Archipelago grass theme

    // Text and icons on strong fills (brand colours, danger swipes)
    val onBrand = white

    // Shades the Material schemes below share
    private val deepSea = Color(0xFF0B2A3A)
    private val deepOchre = Color(0xFF3A3000)
    private val foam = Color(0xFFF3FAFB)
    private val mist = Color(0xFFE6EEF7)

    /** Material roles for the light theme; screens read them through MaterialTheme, never directly. */
    val lightScheme =
        lightColorScheme(
            primary = teal,
            onPrimary = onBrand,
            primaryContainer = sky,
            onPrimaryContainer = deepSea,
            secondary = Color(0xFF4F8A98),
            onSecondary = onBrand,
            secondaryContainer = mint,
            onSecondaryContainer = Color(0xFF0F2F36),
            tertiary = Color(0xFF8A6D00),
            onTertiary = onBrand,
            tertiaryContainer = butter,
            onTertiaryContainer = deepOchre,
            background = foam,
            onBackground = navy,
            surface = foam,
            onSurface = navy,
            surfaceVariant = mint,
            onSurfaceVariant = Color(0xFF2F4858),
            outline = Color(0xFF699CA8),
            outlineVariant = Color(0xFFB7D4D8),
            surfaceContainerLowest = white,
            surfaceContainerLow = Color(0xFFF5FBFC),
            surfaceContainer = Color(0xFFEEF8F9),
            surfaceContainerHigh = Color(0xFFE9F5F7),
            surfaceContainerHighest = Color(0xFFE3F2F4),
        )

    /** Material roles for the dark theme. */
    val darkScheme =
        darkColorScheme(
            primary = sky,
            onPrimary = deepSea,
            primaryContainer = teal,
            onPrimaryContainer = onBrand,
            secondary = butter,
            onSecondary = deepOchre,
            secondaryContainer = Color(0xFF52501F),
            onSecondaryContainer = butter,
            tertiary = grass,
            onTertiary = Color(0xFF003912),
            tertiaryContainer = Color(0xFF1F6B2A),
            onTertiaryContainer = Color(0xFFB5E9A4),
            background = Color(0xFF0E1C33),
            onBackground = mist,
            surface = navy,
            onSurface = mist,
            surfaceVariant = Color(0xFF1F3A5C),
            onSurfaceVariant = Color(0xFFB8CCE0),
            outline = Color(0xFF83A8E1),
            outlineVariant = Color(0xFF4C658B),
            surfaceContainerLowest = Color(0xFF0B1729),
            surfaceContainerLow = Color(0xFF142A44),
            surfaceContainer = Color(0xFF183049),
            surfaceContainerHigh = Color(0xFF1D3556),
            surfaceContainerHighest = Color(0xFF223C61),
        )

    // Quest state: the list dot, the map pin's body, park outlines and trails all agree, so a glance shows what is done and what is
    // left. Doable is blue; never red (errors), amber (in progress) or green (done).
    val questTodo = Color(0xFF0277BD)
    val questProgress = Color(0xFFF9A825)
    val questDone = Color(0xFF2E7D32)
    val questLocked = Color(0xFF9E9E9E)
    val questHidden = Color(0xFFBDBDBD)

    // Status text and quest-state bars drawn on a card or panel: a darker shade on light, a lighter one on dark, so each reads at
    // WCAG 4.5:1 (text) or 3:1 (bars) on both themes (ContrastTest). The map keeps the plain state colours above.
    private val successLight = Color(0xFF1B6E20)
    private val successDark = Color(0xFF81C784)
    private val warningLight = Color(0xFFA33A00)
    private val warningDark = Color(0xFFFFB74D)
    private val dangerLight = Color(0xFFB71C1C)
    private val dangerDark = Color(0xFFFF8A80)
    private val mutedLight = Color(0xFF2F4858)
    private val mutedDark = Color(0xFFB8CCE0)
    private val todoDark = Color(0xFF4FC3F7)
    private val progressLight = Color(0xFFA35F00)
    private val progressDark = Color(0xFFFFC76B)
    private val lockedLight = Color(0xFF616161)
    private val lockedDark = Color(0xFFBDBDBD)

    // Difficulty pill on the quest card: green, amber, purple, lighter on dark so they stay readable.
    private val easy = Color(0xFF1B6E20)
    private val medium = Color(0xFF8A5000)
    private val hard = Color(0xFF6A1B9A)
    private val easyDark = Color(0xFF8FD694)
    private val mediumDark = Color(0xFFFFC76B)
    private val hardDark = Color(0xFFD6A6FF)

    // Pickups (forager items): one cream for all, outlined in navy, so they never share a colour with a quest state
    val pickup = Color(0xFFFFF1CC)

    // A quest's tool badge: navy once the tool is held, orange while it is still missing (never a quest-state grey or blue)
    val toolHeld = navy
    val toolMissing = Color(0xFFE65100)

    // Map and realm editor
    val me = Color(0xFF1565C0)
    val trace = Color(0xFFB39DDB) // where you walked: pale lavender, solid (transparency would darken overlaps); no quest state uses it
    val meUncertain = Color(0xFF90A4AE) // a pin whose position is old: greyed blue
    val realm = Color(0xFF78858C) // a neutral grey: the boundary is background, and blue is kept for you
    val home = Color(0xFF2E7D32)
    val draft = Color(0xFFEF6C00) // the shape being edited
    val draftStrong = Color(0xFFBF360C) // radius line and label
    val thaw = Color(0xFF00ACC1)
    val waypoint = Color(0xFF8E24AA)
    val onMap = white // halos, outlines and knob fills
    val onPin = white // a pin's ring, glyph and pips

    // Vector icons are drawn in this and tinted where they are shown (Icon tint), so it never reaches the screen.
    val iconStroke = Color.Black

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
            Color(0xFF5D4037) to listOf("bus_stop_bingo", "all_aboard", "rack_em_up"), // getting around
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

    fun difficulty(
        d: String,
        dark: Boolean,
    ): Color =
        when (d) {
            "easy" -> if (dark) easyDark else easy
            "hard" -> if (dark) hardDark else hard
            else -> if (dark) mediumDark else medium
        }

    fun tone(
        t: Tone,
        dark: Boolean,
    ): Color =
        when (t) {
            Tone.Success -> if (dark) successDark else successLight
            Tone.Warning -> if (dark) warningDark else warningLight
            Tone.Danger -> if (dark) dangerDark else dangerLight
            Tone.Muted -> if (dark) mutedDark else mutedLight
        }

    /** [tone] for the current theme. */
    @Composable
    @ReadOnlyComposable
    fun tone(t: Tone): Color = tone(t, isSystemInDarkTheme())

    /** [questOnCard] for the current theme. */
    @Composable
    @ReadOnlyComposable
    fun questOnCard(state: String): Color = questOnCard(state, isSystemInDarkTheme())

    /** A quest state's colour for a bar or text on a card (see [quest] for the map). */
    fun questOnCard(
        state: String,
        dark: Boolean,
    ): Color =
        when (state) {
            "done" -> if (dark) successDark else successLight
            "progress" -> if (dark) progressDark else progressLight
            "locked", "hidden" -> if (dark) lockedDark else lockedLight
            else -> if (dark) todoDark else questTodo
        }

    /** A backpack slot past the bag size (#142): the in-progress amber, readable on both themes (ContrastTest). */
    fun overload(dark: Boolean): Color = if (dark) progressDark else progressLight

    /** The icon on a quest badge filled with [quest]: navy on the light amber and greys, white on the rest. */
    fun badgeInk(state: String): Color = inkOn(quest(state))

    /** Navy or white, whichever reads better on [fill]. */
    fun inkOn(fill: Color): Color = if (Contrast.ratio(navy, fill) >= Contrast.ratio(onPin, fill)) navy else onPin

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
