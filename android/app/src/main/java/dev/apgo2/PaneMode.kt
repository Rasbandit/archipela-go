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
}
