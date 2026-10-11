package dev.apgo2

/** The "go to my location" button: where the camera ends up. */
internal object MapLocate {
    /** Street level, where pickups are full tokens: a jump to you from further out zooms in to here. */
    const val STREET_ZOOM = 16.0

    /** The zoom after a jump to you: in to [STREET_ZOOM] from further out, kept when already closer. */
    fun zoom(current: Double): Double = maxOf(current, STREET_ZOOM)
}
