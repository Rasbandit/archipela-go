package dev.apgo2

/**
 * How little the map has to move so a point and its callout are fully in view: nothing when they already are. The result is how far
 * the view moves in screen pixels (the map content moves the other way).
 */
internal object FocusShift {
    /** A map [width] x [height] with [top] and [bottom] covered by overlays, keeping things [margin] clear of every visible edge. */
    data class View(
        val width: Float,
        val height: Float,
        val top: Float,
        val bottom: Float,
        val margin: Float,
    )

    /** The view shift (x, y) that brings the point at [x], [y], [above] of callout over it and [below] of pin under it, into [v]. */
    fun needed(
        x: Float,
        y: Float,
        above: Float,
        below: Float,
        v: View,
    ): Pair<Float, Float> {
        val dx =
            when {
                x < v.margin -> x - v.margin
                x > v.width - v.margin -> x - (v.width - v.margin)
                else -> 0f
            }
        val minTop = v.top + v.margin
        val maxBottom = v.height - v.bottom - v.margin
        val blockTop = y - above
        val blockBottom = y + below
        val dy =
            when {
                blockTop < minTop -> blockTop - minTop

                // Lift it, but never so far that the callout's top leaves the view.
                blockBottom > maxBottom -> minOf(blockBottom - maxBottom, blockTop - minTop)

                else -> 0f
            }
        return dx to dy
    }

    /** The view shift (x, y) that puts the middle of the point with its callout and pin (as in [needed]) in the middle of [v]. */
    fun centred(
        x: Float,
        y: Float,
        above: Float,
        below: Float,
        v: View,
    ): Pair<Float, Float> {
        val blockMiddle = y + (below - above) / 2
        val visibleMiddle = v.top + (v.height - v.top - v.bottom) / 2
        return x - v.width / 2 to blockMiddle - visibleMiddle
    }

    /** Whether [shift] is more than a screen: then the point is far away and is better centred than nudged to an edge. */
    fun far(
        shift: Pair<Float, Float>,
        v: View,
    ): Boolean = kotlin.math.abs(shift.first) > v.width || kotlin.math.abs(shift.second) > v.height
}
