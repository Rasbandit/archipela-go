package dev.apgo2.ui

import androidx.compose.ui.platform.UriHandler
import dev.apgo2.Diag
import uniffi.apgo_ffi.PoiDetailsOut

/** One line of a place's details card; [link] opens when tapped. */
internal data class PoiRow(
    val label: String,
    val value: String,
    val link: String? = null,
)

/** A place's details as card rows: only what the map knows, in a fixed order. The core already filtered links to http(s). */
internal object PoiFormat {
    /** Open [link] with [handler]; false when no app on the device can (no dialer on a tablet, no browser), instead of throwing. */
    fun open(
        handler: UriHandler,
        link: String,
    ): Boolean = runCatching { handler.openUri(link) }.onFailure { Diag.error("PoiFormat", "no app opens $link", it) }.isSuccess

    fun title(d: PoiDetailsOut): String = listOfNotNull(d.name, d.typeLabel).joinToString(" · ").ifEmpty { "Place" }

    fun rows(d: PoiDetailsOut): List<PoiRow> =
        listOfNotNull(
            d.description?.let { PoiRow("About", it) },
            d.address?.let { PoiRow("Address", it) },
            d.openingHours?.let { PoiRow("Hours", it) },
            d.phone?.let { PoiRow("Phone", it, "tel:" + it.filter { c -> c.isDigit() || c == '+' }) },
            d.website?.let { PoiRow("Website", it, it) },
            d.surface?.let { PoiRow("Surface", it) },
            d.length?.let { PoiRow("Length", it) },
            d.lit?.let { PoiRow("Lit", it) },
            d.difficulty?.let { PoiRow("Difficulty", it) },
            d.wikipediaUrl?.let { PoiRow("Wikipedia", "Read the article", it) },
            d.osmUrl?.let { PoiRow("OpenStreetMap", "View on the map", it) },
        )
}
