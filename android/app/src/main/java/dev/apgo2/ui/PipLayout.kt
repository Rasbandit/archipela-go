package dev.apgo2.ui

/** Where a quest pin's difficulty dots go: a row of one or two, or for three a triangle pointing down like the pin's point. */
internal object PipLayout {
    private const val MAX_PIPS = 3
    private const val TRIANGLE_HEIGHT = 0.8660254f // sin 60°: the third dot sits below the middle of the first two

    /** The centres of [pips] dots [gap] apart, the top row on [top], centred on [cx]. */
    fun centers(
        pips: Int,
        cx: Float,
        top: Float,
        gap: Float,
    ): List<Pair<Float, Float>> =
        when (pips.coerceIn(0, MAX_PIPS)) {
            0 -> emptyList()
            1 -> listOf(cx to top)
            2 -> listOf(cx - gap / 2 to top, cx + gap / 2 to top)
            else -> listOf(cx - gap / 2 to top, cx + gap / 2 to top, cx to top + gap * TRIANGLE_HEIGHT)
        }
}
