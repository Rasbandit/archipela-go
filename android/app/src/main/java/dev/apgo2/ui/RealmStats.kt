package dev.apgo2.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** One figure with its label under it. */
@Composable
private fun RowScope.Stat(label: String, value: String, modifier: Modifier = Modifier) {
    Column(modifier.weight(1f)) {
        Text(value, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold, maxLines = 1)
        Text(label, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
    }
}

/**
 * What the player is choosing, in numbers: how big it is and how far it reaches from home, and (once scanned) what there is to do in it.
 * A figure that is not known yet is null and shows [waiting] ("after scan" or "looking…") in its place.
 */
@Composable
fun RealmStatsBox(area: Double, farthest: Double, scan: ScanFigures?, waiting: String) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Stat("area", Units.area(area))
            Stat("farthest from home", Units.distance(farthest))
            Stat("walkable", scan?.let { Units.distance(it.walkableM) } ?: waiting)
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Stat("streets", scan?.streets?.toString() ?: waiting)
            Stat("trails", scan?.let { Units.distance(it.trailM) } ?: waiting)
            Stat("finds", scan?.finds?.toString() ?: waiting)
        }
    }
}

/** The figures a scan gives (plain values, so this file does not depend on the generated bindings). */
data class ScanFigures(val walkableM: Double, val streets: Int, val trailM: Double, val finds: Int)
