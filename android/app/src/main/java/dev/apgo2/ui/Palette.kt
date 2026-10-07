package dev.apgo2.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb

/** "#rrggbb" for MapLibre style expressions, which take strings rather than Compose colours. */
fun Color.hex(): String = "#%06x".format(toArgb() and 0xFFFFFF)

/**
 * Every colour the app uses lives here, so Compose screens and the map draw from one source.
 * Brand colours come from Archipelago's web theme (ArchipelagoMW/Archipelago, WebHostLib/static/styles, MIT);
 * only colours are used, the Archipelago logo is CC BY-NC 4.0 and is deliberately not bundled.
 */
object ApgoPalette {
    // Archipelago ocean theme (#11233e panels, #93dcff headings, #fffc95 links) and header teals
    val navy = Color(0xFF11233E)
    val teal = Color(0xFF2F6B83)
    val sky = Color(0xFF93DCFF)
    val mint = Color(0xFFD0EBE6)
    val butter = Color(0xFFFFFC95)
    val grass = Color(0xFF5AFF6A) // Archipelago grass theme

    // Quest state: the list dot and the map marker must agree
    val questTodo = Color(0xFFD32F2F)
    val questProgress = Color(0xFFF9A825)
    val questDone = Color(0xFF2E7D32)
    val questLocked = Color(0xFF9E9E9E)
    val questHidden = Color(0xFFBDBDBD)

    fun quest(state: String): Color = when (state) {
        "done" -> questDone
        "progress" -> questProgress
        "locked" -> questLocked
        "hidden" -> questHidden
        else -> questTodo
    }

    // Map and realm editor
    val me = Color(0xFF1565C0)
    val realm = Color(0xFF1565C0)
    val home = Color(0xFF2E7D32)
    val draft = Color(0xFFEF6C00) // the shape being edited
    val draftStrong = Color(0xFFBF360C) // radius line and label
    val thaw = Color(0xFF00ACC1)
    val waypoint = Color(0xFF8E24AA)
    val onMap = Color.White // halos, outlines and knob fills

    // Marks on places
    val favorite = Color(0xFFF9A825)
    val banned = Color(0xFFC62828)

    // Feedback text
    val warning = Color(0xFFE65100)
    val danger = Color(0xFFC62828)
    val success = Color(0xFF2E7D32)
    val muted = Color(0xFF757575)
}
