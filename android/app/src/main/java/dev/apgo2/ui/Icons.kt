package dev.apgo2.ui

import androidx.compose.ui.graphics.vector.ImageVector
import com.composables.icons.lucide.Ban
import com.composables.icons.lucide.Bike
import com.composables.icons.lucide.Car
import com.composables.icons.lucide.Check
import com.composables.icons.lucide.Flag
import com.composables.icons.lucide.Footprints
import com.composables.icons.lucide.Lock
import com.composables.icons.lucide.Lucide
import com.composables.icons.lucide.MapPinned
import com.composables.icons.lucide.Plus
import com.composables.icons.lucide.RefreshCw
import com.composables.icons.lucide.Sparkles
import com.composables.icons.lucide.Star
import com.composables.icons.lucide.Trash2
import com.composables.icons.lucide.TriangleAlert
import com.composables.icons.lucide.Zap

/**
 * Every icon the app shows, named by what it means. Screens use these, never the icon library directly,
 * so the set can be swapped in one place. Lucide (https://lucide.dev, ISC licence, see THIRD_PARTY_NOTICES.md).
 */
object ApgoIcons {
    // Navigation
    val Realms = Lucide.MapPinned
    val NewGame = Lucide.Sparkles
    val Play = Lucide.Flag

    // Marks on places
    val Favorite = Lucide.Star
    val Banned = Lucide.Ban

    // Travel modes
    val Walk = Lucide.Footprints
    val Run = Lucide.Zap
    val Bike = Lucide.Bike
    val Car = Lucide.Car

    // Actions
    val Add = Lucide.Plus
    val Delete = Lucide.Trash2
    val Rescan = Lucide.RefreshCw

    // Status
    val Warning = Lucide.TriangleAlert
    val Unlocked = Lucide.Check
    val Locked = Lucide.Lock

    fun mode(mode: String): ImageVector = when (mode) {
        "run" -> Run
        "bike" -> Bike
        "drive" -> Car
        else -> Walk
    }
}
