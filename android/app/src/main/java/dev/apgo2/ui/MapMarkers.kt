package dev.apgo2.ui

import android.graphics.Bitmap

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
}

/**
 * The one definition of a map pin, used by the realm editor and the Play map so they cannot drift apart: what it looks like
 * ([render]), what each quest state looks like ([badge]), how big it is ([iconScale]) and in which order pins win a collision
 * ([drawOrder]).
 */
internal object MapMarkers {
    private const val KEY_PARTS = 4
    private const val FIND_PIN_PX = 144
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

    enum class Badge { None, Progress, Done, Locked }

    /** Pixel size of a quest pin's bitmap; [iconScale] is the factor the map draws it at. */
    const val QUEST_PIN_PX = 160

    fun parse(key: String): MarkerSpec? {
        val p = key.split("|")
        if (p.size != KEY_PARTS) return null
        val (kindId, family, extra) = p.drop(1)
        return when (p[0]) {
            "pin" -> MarkerSpec.Find(kindId, family, extra)
            "quest" -> MarkerSpec.Quest(kindId, family, extra)
            else -> null
        }
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
                        renderPin(
                            icon,
                            FIND_PIN_PX,
                            fill = fill,
                            ring = ApgoPalette.favorite,
                            ringFraction = FAVORITE_RING_FRACTION,
                        )
                    }

                    "banned" -> {
                        renderPin(icon, FIND_PIN_PX, fill = ApgoPalette.muted)
                    }

                    else -> {
                        renderPin(icon, FIND_PIN_PX, fill = fill)
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
        }
}
