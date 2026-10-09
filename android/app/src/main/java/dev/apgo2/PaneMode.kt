package dev.apgo2

/** Play's panel is either shown (under the map) or hidden (only its grip, goal and summary line stay); a drag snaps between them. */
internal object PaneMode {
    /** Whether the panel is shown after the grip was dragged [dragPx] (down is positive): only a drag past [thresholdPx] flips it. */
    fun afterDrag(
        shown: Boolean,
        dragPx: Float,
        thresholdPx: Float,
    ): Boolean =
        when {
            dragPx > thresholdPx -> false
            dragPx < -thresholdPx -> true
            else -> shown
        }

    /**
     * How open the panel's lower part is (1 shown, 0 hidden) after the grip moved [dyPx] (down is positive) during a drag: it follows
     * the finger over the part's height [bodyPx] and stops at either end.
     */
    fun openAfterMove(
        open: Float,
        dyPx: Float,
        bodyPx: Float,
    ): Float = if (bodyPx <= 0f) open else (open - dyPx / bodyPx).coerceIn(0f, 1f)
}
