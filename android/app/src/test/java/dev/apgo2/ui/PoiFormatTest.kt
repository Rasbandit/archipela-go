package dev.apgo2.ui

import androidx.compose.ui.platform.UriHandler
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.apgo_ffi.PoiDetailsOut

class PoiFormatTest {
    private fun details(
        name: String? = null,
        typeLabel: String? = null,
        address: String? = null,
        openingHours: String? = null,
        phone: String? = null,
        website: String? = null,
        wikipediaUrl: String? = null,
        description: String? = null,
        surface: String? = null,
        length: String? = null,
        lit: String? = null,
        difficulty: String? = null,
        osmUrl: String? = null,
    ) = PoiDetailsOut(
        name,
        typeLabel,
        address,
        openingHours,
        phone,
        website,
        wikipediaUrl,
        description,
        surface,
        length,
        lit,
        difficulty,
        osmUrl,
    )

    @Test fun onlyKnownFieldsBecomeRowsInOrder() {
        val rows =
            PoiFormat.rows(
                details(address = "12 Main St", openingHours = "Mo-Fr 07:00-17:00", surface = "Gravel", length = "1.2 km"),
            )
        assertEquals(listOf("Address", "Hours", "Surface", "Length"), rows.map { it.label })
        assertEquals(listOf(null, null, null, null), rows.map { it.link })
    }

    @Test fun linksOpenTheRightPlaces() {
        val rows =
            PoiFormat.rows(
                details(
                    phone = "+1 555 0100",
                    website = "https://a.example",
                    wikipediaUrl = "https://en.wikipedia.org/wiki/X",
                    osmUrl = "https://www.openstreetmap.org/node/1",
                ),
            )
        assertEquals(
            listOf(
                PoiRow("Phone", "+1 555 0100", "tel:+15550100"),
                PoiRow("Website", "https://a.example", "https://a.example"),
                PoiRow("Wikipedia", "Read the article", "https://en.wikipedia.org/wiki/X"),
                PoiRow("OpenStreetMap", "View on the map", "https://www.openstreetmap.org/node/1"),
            ),
            rows,
        )
    }

    @Test fun nothingKnownGivesNoRows() {
        assertEquals(emptyList<PoiRow>(), PoiFormat.rows(details()))
    }

    @Test fun titleUsesNameThenTypeThenAFallback() {
        assertEquals("Bean There · Cafe", PoiFormat.title(details(name = "Bean There", typeLabel = "Cafe")))
        assertEquals("Cafe", PoiFormat.title(details(typeLabel = "Cafe")))
        assertEquals("Place", PoiFormat.title(details()))
    }

    @Test fun openingALinkNoAppHandlesReportsFailureInsteadOfThrowing() {
        val none =
            object : UriHandler {
                override fun openUri(uri: String) = throw IllegalArgumentException("no activity for $uri")
            }
        var opened: String? = null
        val browser =
            object : UriHandler {
                override fun openUri(uri: String) {
                    opened = uri
                }
            }
        assertFalse(PoiFormat.open(none, "tel:+15550100"))
        assertTrue(PoiFormat.open(browser, "https://a.example"))
        assertEquals("https://a.example", opened)
    }
}
