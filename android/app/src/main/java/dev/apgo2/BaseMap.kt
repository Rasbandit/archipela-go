package dev.apgo2

import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillExtrusionLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.Layer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory.visibility
import org.maplibre.android.style.layers.SymbolLayer

/**
 * Trims the base map (OpenFreeMap's style) so it does not compete with the game: its own place icons (shops, parking, toilets,
 * bus stops, peaks, airports) are hidden, since our pins are the places that matter. Streets, names and areas stay. Buildings are
 * drawn flat: the style's 3D extrusions cost GPU on every frame at street zoom, where the map is used most.
 */
internal object BaseMap {
    // OpenMapTiles source layers whose symbols are icons for places.
    private val HIDDEN = setOf("poi", "mountain_peak", "aerodrome_label")

    // Layer types never drawn.
    private val HIDDEN_TYPES = setOf("fill-extrusion")

    // The flat building footprint stops where the style's 3D buildings take over; it carries on up to the deepest zoom instead.
    private const val FLAT_BUILDINGS_MAX_ZOOM = 24f

    /** Whether the base map's layers drawn from [sourceLayer] are hidden. */
    fun hides(sourceLayer: String?): Boolean = sourceLayer in HIDDEN

    /** Whether the base map's layers of this [type] (a style JSON layer type) are hidden. */
    fun hidesType(type: String): Boolean = type in HIDDEN_TYPES

    /** Whether this layer is the flat building footprint, kept on at every zoom in place of the 3D buildings. */
    fun flatBuildings(
        type: String,
        sourceLayer: String?,
    ): Boolean = type == "fill" && sourceLayer == "building"

    /** Hide those layers in a freshly loaded [style], and draw buildings flat. */
    fun trim(style: Style) {
        style.layers.forEach {
            val type = typeOf(it)
            when {
                hides(sourceLayerOf(it)) || hidesType(type) -> it.setProperties(visibility(Property.NONE))
                flatBuildings(type, sourceLayerOf(it)) -> it.maxZoom = FLAT_BUILDINGS_MAX_ZOOM
            }
        }
    }

    private fun typeOf(layer: Layer): String =
        when (layer) {
            is FillLayer -> "fill"
            is FillExtrusionLayer -> "fill-extrusion"
            else -> ""
        }

    private fun sourceLayerOf(layer: Layer): String? =
        when (layer) {
            is SymbolLayer -> layer.sourceLayer
            is CircleLayer -> layer.sourceLayer
            is FillLayer -> layer.sourceLayer
            else -> null
        }
}
