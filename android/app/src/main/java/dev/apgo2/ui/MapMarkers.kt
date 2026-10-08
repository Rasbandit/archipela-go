package dev.apgo2.ui

import android.graphics.Bitmap

/** What a map pin shows. The [key] names its bitmap in the map style, so equal specs share one image. */
sealed interface MarkerSpec {
    val key: String

    /** A scanned place in the realm editor: family colour, with a ring for favorites and grey for banned. [mark] is none | favorite | banned. */
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
 * ([render]), what each quest state looks like ([badge]), how big it is ([iconScale]) and in which order pins win a collision ([drawOrder]).
 */
object MapMarkers {
    enum class Badge { None, Progress, Done, Locked }

    /** Pixel size of a quest pin's bitmap; [iconScale] is the factor the map draws it at. */
    const val QUEST_PIN_PX = 120

    fun parse(key: String): MarkerSpec? {
        val p = key.split("|")
        if (p.size != 4) return null
        return when (p[0]) {
            "pin" -> MarkerSpec.Find(p[1], p[2], p[3])
            "quest" -> MarkerSpec.Quest(p[1], p[2], p[3])
            else -> null
        }
    }

    fun badge(state: String): Badge =
        when (state) {
            "progress" -> Badge.Progress
            "done" -> Badge.Done
            "locked" -> Badge.Locked
            else -> Badge.None
        }

    /** Easy < medium < hard < boss. */
    fun iconScale(
        difficulty: String,
        boss: Boolean,
    ): Float =
        when {
            boss -> 0.85f
            difficulty.equals("easy", ignoreCase = true) -> 0.55f
            difficulty.equals("hard", ignoreCase = true) -> 0.7f
            else -> 0.62f
        }

    /** Lower draws and claims space first: what you can act on beats what is done or out of reach. */
    fun drawOrder(state: String): Int =
        when (state) {
            "progress" -> 0
            "open" -> 1
            "locked" -> 2
            "done" -> 3
            else -> 4
        }

    fun render(spec: MarkerSpec): Bitmap =
        when (spec) {
            is MarkerSpec.Find -> {
                val icon = ApgoIcons.forKind(spec.kindId, spec.family)
                val fill = ApgoPalette.kind(spec.kindId, spec.family)
                when (spec.mark) {
                    "favorite" -> renderPin(icon, 96, fill = fill, ring = ApgoPalette.favorite, ringFraction = 0.13f)
                    "banned" -> renderPin(icon, 96, fill = ApgoPalette.muted)
                    else -> renderPin(icon, 96, fill = fill)
                }
            }

            is MarkerSpec.Quest -> {
                val locked = spec.state == "locked"
                renderQuestPin(
                    ApgoIcons.forKind(spec.kindId, spec.family),
                    QUEST_PIN_PX,
                    fill = if (locked) ApgoPalette.muted else ApgoPalette.kind(spec.kindId, spec.family),
                    badge = badge(spec.state),
                )
            }
        }
}
