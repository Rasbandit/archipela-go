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

    /** A quest on the Play map: family colour, and its state as a corner badge. [state] is open | progress | done | locked. */
    data class Quest(
        val kindId: String,
        val family: String,
        val state: String,
    ) : MarkerSpec {
        override val key get() = "quest|$kindId|$family|$state"
    }

    /** A cluster of quests: a ring split by how many are in each of [MapMarkers.RING_STATES], around the count. */
    data class Ring(
        val shares: List<Int>,
    ) : MarkerSpec {
        override val key get() = "ring|" + shares.joinToString("|")
    }
}

/**
 * The one definition of a map pin, used by the realm editor and the Play map so they cannot drift apart: what it looks like
 * ([render]), what each quest state looks like ([badge]), how big it is ([iconScale]) and in which order pins win a collision
 * ([drawOrder]).
 */
internal object MapMarkers {
    private const val KEY_PARTS = 4
    private const val FIND_PIN_PX = 180
    private const val FAVORITE_RING_FRACTION = 0.13f

    // At most 1: a bigger factor would blur the bitmap. The boss is drawn at full size.
    private const val SCALE_BOSS = 1f
    private const val SCALE_EASY = 0.62f
    private const val SCALE_MEDIUM = 0.7f
    private const val SCALE_HARD = 0.78f
    private const val ORDER_DONE = 3
    private const val ORDER_OTHER = 4
    private const val STATE_PROGRESS = "progress"
    private const val STATE_LOCKED = "locked"
    private const val STATE_DONE = "done"
    private const val FULL_TURN = 360f

    enum class Badge { None, Progress, Done, Locked }

    /** Pixel size of a quest pin's bitmap; [iconScale] is the factor the map draws it at. */
    const val QUEST_PIN_PX = 200

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
    fun selectedQuestPinHeightPx(
        difficulty: String,
        boss: Boolean,
    ): Float = QUEST_PIN_PX * PIN_HEIGHT_RATIO * iconScale(difficulty, boss) * SELECTED_GROWTH

    /** On-screen height of the selected find's pin. */
    fun selectedFindPinHeightPx(): Float = FIND_PIN_PX * PIN_HEIGHT_RATIO * FIND_SELECTED_SIZE

    fun parse(key: String): MarkerSpec? {
        val p = key.split("|")
        val rest = p.drop(1)
        return when {
            p[0] == "ring" -> parseRing(rest)
            p.size != KEY_PARTS -> null
            p[0] == "pin" -> MarkerSpec.Find(rest[0], rest[1], rest[2])
            p[0] == "quest" -> MarkerSpec.Quest(rest[0], rest[1], rest[2])
            else -> null
        }
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

    fun badge(state: String): Badge =
        when (state) {
            STATE_PROGRESS -> Badge.Progress
            STATE_DONE -> Badge.Done
            STATE_LOCKED -> Badge.Locked
            else -> Badge.None
        }

    /** Easy < medium < hard < boss. */
    fun iconScale(
        difficulty: String,
        boss: Boolean,
    ): Float =
        when {
            boss -> SCALE_BOSS
            difficulty.equals("easy", ignoreCase = true) -> SCALE_EASY
            difficulty.equals("hard", ignoreCase = true) -> SCALE_HARD
            else -> SCALE_MEDIUM
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
                val locked = spec.state == STATE_LOCKED
                renderQuestPin(
                    ApgoIcons.forKind(spec.kindId, spec.family),
                    QUEST_PIN_PX,
                    fill = if (locked) ApgoPalette.muted else ApgoPalette.kind(spec.kindId, spec.family),
                    badge = badge(spec.state),
                )
            }

            is MarkerSpec.Ring -> {
                renderRing(ringSegments(spec), RING_PX)
            }
        }
}
