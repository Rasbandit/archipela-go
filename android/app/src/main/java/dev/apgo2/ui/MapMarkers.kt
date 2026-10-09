package dev.apgo2.ui

import android.graphics.Bitmap
import androidx.compose.ui.graphics.Color

/** What a map pin shows. The [key] names its bitmap in the map style, so equal specs share one image. */
internal sealed interface MarkerSpec {
    val key: String

    /**
     * A scanned place in the realm editor: family colour, with a ring for favorites and grey for banned. [mark] is none |
     * favorite | banned.
     */
    data class Find(
        val kindId: String,
        val family: String,
        val mark: String,
    ) : MarkerSpec {
        override val key get() = "pin|$kindId|$family|$mark"
    }

    /**
     * A quest on the Play map: its body in the state colour (open | progress | done | locked), its kind's icon, and [pips] (1 to 3)
     * dots for its difficulty.
     */
    data class Quest(
        val kindId: String,
        val family: String,
        val state: String,
        val pips: Int,
    ) : MarkerSpec {
        override val key get() = "quest|$kindId|$family|$state|$pips"
    }

    /** A cluster of quests: a ring split by how many are in each of [MapMarkers.RING_STATES], around the count. */
    data class Ring(
        val shares: List<Int>,
    ) : MarkerSpec {
        override val key get() = "ring|" + shares.joinToString("|")
    }

    /** A forager item on the Play map: its theme's icon on a body in its quest's state colour (grey while locked), no pips. */
    data class Item(
        val theme: String,
        val state: String,
    ) : MarkerSpec {
        override val key get() = "item|$theme|$state"
    }
}

/**
 * The one definition of a map pin, used by the realm editor and the Play map so they cannot drift apart: what it looks like
 * ([render]), what each quest state looks like ([badge]), how big it is ([QUEST_SCALE]) and in which order pins win a collision
 * ([drawOrder]).
 */
internal object MapMarkers {
    private const val KEY_PARTS = 4
    private const val FIND_PIN_PX = 180
    private const val FAVORITE_RING_FRACTION = 0.13f

    private const val ORDER_DONE = 3
    private const val ORDER_OTHER = 4
    private const val STATE_PROGRESS = "progress"
    private const val STATE_LOCKED = "locked"
    private const val STATE_DONE = "done"
    private const val FULL_TURN = 360f
    private const val QUEST_PARTS = 4
    private const val MAX_PIPS = 3

    enum class Badge { None, Progress, Locked }

    /** Pixel size of a quest pin's bitmap; [QUEST_SCALE] is the factor the map draws it at. */
    const val QUEST_PIN_PX = 200

    /**
     * Every quest pin is drawn at this one size (at most 1: more would blur the bitmap). Difficulty is not shown by size, so
     * neighbouring pins never look mismatched.
     */
    const val QUEST_SCALE = 0.78f

    /** Pin images are this many times as tall as wide: the head, then the point below it that marks the spot. */
    const val PIN_HEIGHT_RATIO = 1.3f

    /** How much bigger the selected quest pin is drawn. */
    const val SELECTED_GROWTH = 1.25f

    /** The draw size factor of a find pin, and of the selected one. */
    const val FIND_SIZE = 0.62f
    const val FIND_SELECTED_SIZE = 0.92f

    /** The states a cluster ring shows, in drawing order (clockwise from the top): what you can act on first. */
    val RING_STATES = listOf(STATE_PROGRESS, "open", STATE_LOCKED, STATE_DONE)

    /** Bitmap size of a cluster ring. */
    const val RING_PX = 112

    // A map image is drawn at its own pixel size times its size factor (its bitmap density is the screen's).

    /** On-screen height of the selected quest's pin, head to point: what a callout above the point must clear. */
    fun selectedQuestPinHeightPx(): Float = QUEST_PIN_PX * PIN_HEIGHT_RATIO * QUEST_SCALE * SELECTED_GROWTH

    /** On-screen height of the selected find's pin. */
    fun selectedFindPinHeightPx(): Float = FIND_PIN_PX * PIN_HEIGHT_RATIO * FIND_SELECTED_SIZE

    fun parse(key: String): MarkerSpec? {
        val p = key.split("|")
        val rest = p.drop(1)
        return when {
            p[0] == "ring" -> parseRing(rest)
            p[0] == "quest" -> parseQuest(rest)
            p[0] == "item" -> if (rest.size == 2) MarkerSpec.Item(rest[0], rest[1]) else null
            p.size != KEY_PARTS -> null
            p[0] == "pin" -> MarkerSpec.Find(rest[0], rest[1], rest[2])
            else -> null
        }
    }

    private fun parseQuest(parts: List<String>): MarkerSpec.Quest? {
        val pips = parts.getOrNull(QUEST_PARTS - 1)?.toIntOrNull()
        val valid = parts.size == QUEST_PARTS && pips != null && pips in 1..MAX_PIPS
        return if (valid) MarkerSpec.Quest(parts[0], parts[1], parts[2], pips) else null
    }

    /** Difficulty as dots on the pin: easy 1, medium 2, hard 3; the boss is the hardest. Anything else reads as medium. */
    fun pips(
        difficulty: String,
        boss: Boolean,
    ): Int =
        when {
            boss || difficulty.equals("hard", ignoreCase = true) -> MAX_PIPS
            difficulty.equals("easy", ignoreCase = true) -> 1
            else -> 2
        }

    private fun parseRing(parts: List<String>): MarkerSpec.Ring? {
        val shares = parts.map { it.toIntOrNull() ?: -1 }
        val valid = shares.size == RING_STATES.size && shares.all { it >= 0 } && shares.sum() > 0
        return if (valid) MarkerSpec.Ring(shares) else null
    }

    /** The arcs of a ring, clockwise from the top: each non-empty state's colour and its sweep in degrees (summing to 360). */
    fun ringSegments(ring: MarkerSpec.Ring): List<Pair<Color, Float>> {
        val total = ring.shares.sum().toFloat()
        return RING_STATES.zip(ring.shares).filter { it.second > 0 }.map { (state, n) -> ApgoPalette.quest(state) to FULL_TURN * n / total }
    }

    /** A quest pin's body colour: its state (the icon on it says what kind of quest it is). Locked pins are grey. */
    fun questFill(state: String): Color = if (state == STATE_LOCKED) ApgoPalette.muted else ApgoPalette.quest(state)

    fun badge(state: String): Badge =
        when (state) {
            STATE_PROGRESS -> Badge.Progress
            STATE_LOCKED -> Badge.Locked
            else -> Badge.None
        }

    /** Lower draws and claims space first: what you can act on beats what is done or out of reach. */
    fun drawOrder(state: String): Int =
        when (state) {
            STATE_PROGRESS -> 0
            "open" -> 1
            STATE_LOCKED -> 2
            STATE_DONE -> ORDER_DONE
            else -> ORDER_OTHER
        }

    fun render(spec: MarkerSpec): Bitmap =
        when (spec) {
            is MarkerSpec.Find -> {
                val icon = ApgoIcons.forKind(spec.kindId, spec.family)
                val fill = ApgoPalette.kind(spec.kindId, spec.family)
                when (spec.mark) {
                    "favorite" -> {
                        renderFindPin(
                            icon,
                            FIND_PIN_PX,
                            fill = fill,
                            ring = ApgoPalette.favorite,
                            ringFraction = FAVORITE_RING_FRACTION,
                        )
                    }

                    "banned" -> {
                        renderFindPin(icon, FIND_PIN_PX, fill = ApgoPalette.muted)
                    }

                    else -> {
                        renderFindPin(icon, FIND_PIN_PX, fill = fill)
                    }
                }
            }

            is MarkerSpec.Quest -> {
                renderQuestPin(
                    ApgoIcons.forKind(spec.kindId, spec.family),
                    QUEST_PIN_PX,
                    fill = questFill(spec.state),
                    badge = badge(spec.state),
                    pips = spec.pips,
                )
            }

            is MarkerSpec.Ring -> {
                renderRing(ringSegments(spec), RING_PX)
            }

            is MarkerSpec.Item -> {
                renderQuestPin(ApgoIcons.collectible(spec.theme), QUEST_PIN_PX, fill = questFill(spec.state), badge = Badge.None, pips = 0)
            }
        }
}
