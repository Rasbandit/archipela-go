package dev.apgo2.ui

import uniffi.apgo_ffi.PlaceOut
import kotlin.math.roundToInt

/** Text for the quest card. Pure, so it is unit-tested; what the card says comes from the core. */
internal object CardFormat {
    /** The place row: the name in bold (quoted when it is a title), then what the place is; the kind alone when it has no name. */
    data class Place(
        val name: String?,
        val kind: String?,
    )

    fun place(p: PlaceOut): Place {
        val name = p.name?.let { if (p.quoted) "“$it”" else it }
        val kind = p.kind?.let { if (name == null) it else " · ${it.lowercase()}" }
        return Place(name, kind)
    }

    fun effort(minutes: Double): String = "~${minutes.roundToInt().coerceAtLeast(1)} min"
}
