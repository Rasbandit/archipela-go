package dev.apgo2.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilledIconToggleButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private const val PILL_ALPHA = 0.95f
private const val PILL_CORNER_DP = 24
private const val BADGE_ALPHA = 0.9f

/** A round icon button for a tool strip floating over the map; the selected tool is filled. */
@Composable
internal fun ToolButton(
    icon: ImageVector,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    selected: Boolean = false,
    enabled: Boolean = true,
) {
    FilledIconToggleButton(
        modifier = modifier,
        checked = selected,
        onCheckedChange = { onClick() },
        enabled = enabled,
        colors = toolToggleColors(),
    ) { Icon(icon, contentDescription = description, modifier = Modifier.size(22.dp)) }
}

/** A rounded group of tools floating over the map. Tools in one pill belong together (for example Circle and Polygon: one or the other). */
@Composable
internal fun ToolPill(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(pillModifier(modifier), verticalArrangement = Arrangement.spacedBy(2.dp), content = content)
}

/** Like [ToolPill], laid out in a row. */
@Composable
internal fun ToolPillRow(
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit,
) {
    Row(pillModifier(modifier), horizontalArrangement = Arrangement.spacedBy(2.dp), content = content)
}

@Composable
private fun pillModifier(modifier: Modifier = Modifier) =
    modifier
        .background(MaterialTheme.colorScheme.surface.copy(alpha = PILL_ALPHA), RoundedCornerShape(PILL_CORNER_DP.dp))
        .padding(4.dp)

/** The colours of a tool toggle: plain until selected, then filled with the primary colour. */
@Composable
internal fun toolToggleColors() =
    IconButtonDefaults.filledIconToggleButtonColors(
        containerColor = Color.Transparent,
        contentColor = MaterialTheme.colorScheme.onSurfaceVariant,
        checkedContainerColor = MaterialTheme.colorScheme.primary,
        checkedContentColor = MaterialTheme.colorScheme.onPrimary,
    )

/** A small confirmation chip that floats over a map ("All changes saved"). */
@Composable
internal fun SavedBadge(
    text: String,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier
            .background(MaterialTheme.colorScheme.surface.copy(alpha = BADGE_ALPHA), RoundedCornerShape(12.dp))
            .padding(horizontal = 8.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Icon(ApgoIcons.Saved, contentDescription = null, tint = ApgoPalette.success, modifier = Modifier.size(14.dp))
        Text(text, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
