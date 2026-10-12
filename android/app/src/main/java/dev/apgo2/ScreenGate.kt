package dev.apgo2

/**
 * Skips redrawing what only the screen shows (quest lists, map data, the pin) while the app is off screen, and owes one full
 * redraw for when it comes back. Starts off screen: the process can load a game before (or without) an activity, and the
 * activity's first start shows it. Not thread-safe: call it from the UI thread.
 */
internal class ScreenGate {
    private var visible = false
    private var owed = false

    /** Whether to redraw now; off screen it notes that a redraw is owed instead. */
    fun draw(): Boolean {
        if (!visible) owed = true
        return visible
    }

    fun hide() {
        visible = false
    }

    /** Back on screen: whether a redraw was skipped meanwhile (and is now due). */
    fun show(): Boolean {
        visible = true
        return owed.also { owed = false }
    }
}
