package dev.apgo2

/** The map's padding is the part of it covered by overlays; the camera target sits in the middle of what is left. */
internal object MapPadding {
    /** The screen y of the camera target on a map [heightPx] tall with [topPx] and [bottomPx] covered (at [topPx] if none is left). */
    fun centerY(
        heightPx: Float,
        topPx: Float,
        bottomPx: Float,
    ): Float = topPx + (heightPx - topPx - bottomPx).coerceAtLeast(0f) / 2f
}
