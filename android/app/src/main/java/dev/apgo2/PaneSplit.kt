package dev.apgo2

/** How Play splits its height between the map (top) and the panel under it, as the map's share of the height. */
internal object PaneSplit {
    /** The split Play opens with. */
    const val DEFAULT_MAP = 0.55f

    /** The panel never covers more than this much of the map. */
    const val MIN_MAP = 0.3f

    // A split this close to the peek counts as collapsed.
    private const val NEAR = 0.01f

    /** The largest map share: the panel collapsed to its peek ([peekPx], the handle and a first line) of [totalPx]. */
    fun maxMap(
        totalPx: Float,
        peekPx: Float,
    ): Float = if (totalPx <= 0f) DEFAULT_MAP else (1f - peekPx / totalPx).coerceAtLeast(MIN_MAP)

    /** The map share after the handle moved [deltaPx] (down is positive) in [totalPx], kept between [MIN_MAP] and [max]. */
    fun dragged(
        map: Float,
        deltaPx: Float,
        totalPx: Float,
        max: Float,
    ): Float = if (totalPx <= 0f) map else (map + deltaPx / totalPx).coerceIn(MIN_MAP, max)

    /** A tap on the handle: collapse the panel to its peek, or bring a collapsed one back to the default. */
    fun toggled(
        map: Float,
        max: Float,
    ): Float = if (map >= max - NEAR) DEFAULT_MAP else max
}
