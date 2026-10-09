package dev.apgo2

import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.Layer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory.visibility
import org.maplibre.android.style.layers.SymbolLayer

/**
 * Trims the base map (OpenFreeMap's style) so it does not compete with the game: its own place icons (shops, parking, toilets,
 * bus stops, peaks, airports) are hidden, since our pins are the places that matter. Streets, names and areas stay.
 */
internal object BaseMap {
    // OpenMapTiles source layers whose symbols are icons for places.
    private val HIDDEN = setOf("poi", "mountain_peak", "aerodrome_label")

    /** Whether the base map's layers drawn from [sourceLayer] are hidden. */
    fun hides(sourceLayer: String?): Boolean = sourceLayer in HIDDEN

    /** Hide those layers in a freshly loaded [style]. */
    fun trim(style: Style) {
        style.layers.filter { hides(sourceLayerOf(it)) }.forEach { it.setProperties(visibility(Property.NONE)) }
    }

    private fun sourceLayerOf(layer: Layer): String? =
        when (layer) {
            is SymbolLayer -> layer.sourceLayer
            is CircleLayer -> layer.sourceLayer
            else -> null
        }
}
