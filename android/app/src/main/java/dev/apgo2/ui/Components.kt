package dev.apgo2.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Shared building blocks. Screens compose these; they do not restyle Material widgets themselves. */

val MODES = listOf("walk", "run", "bike", "drive")

/** The game says "car" where the data says "drive". */
fun modeLabel(mode: String) = if (mode == "drive") "car" else mode

/** A selectable chip. The selected one is a solid fill, so it reads clearly against the card behind it. */
@Composable
fun ApgoChip(label: String, selected: Boolean, onClick: () -> Unit, textSize: TextUnit = 12.sp, icon: ImageVector? = null) {
    FilterChip(
        selected = selected,
        onClick = onClick,
        label = { Text(label, fontSize = textSize) },
        leadingIcon = icon?.let { { Icon(it, contentDescription = null, modifier = Modifier.size(16.dp)) } },
        colors = FilterChipDefaults.filterChipColors(
            selectedContainerColor = MaterialTheme.colorScheme.primary,
            selectedLabelColor = MaterialTheme.colorScheme.onPrimary,
            selectedLeadingIconColor = MaterialTheme.colorScheme.onPrimary,
        ),
    )
}

/** One row of single-choice chips, optionally with an icon on each. */
@Composable
fun <T> ChoiceChips(
    options: List<T>, selected: T, onSelect: (T) -> Unit, label: (T) -> String,
    textSize: TextUnit = 12.sp, spacing: Dp = 6.dp, icon: (T) -> ImageVector? = { null },
) {
    Row(horizontalArrangement = Arrangement.spacedBy(spacing)) {
        options.forEach { ApgoChip(label(it), it == selected, { onSelect(it) }, textSize, icon(it)) }
    }
}

@Composable
fun ModeChips(selected: String, onSelect: (String) -> Unit) = ChoiceChips(MODES, selected, onSelect, ::modeLabel, icon = ApgoIcons::mode)

/** A panel floating over a full-page map. */
@Composable
fun MapOverlayCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Card(modifier.fillMaxWidth().padding(8.dp)) {
        Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp), content = content)
    }
}

enum class Tone { Warning, Danger, Success, Muted }

/** A line of status text; warnings and errors get a warning icon in front. */
@Composable
fun FeedbackText(text: String, tone: Tone, size: TextUnit = 12.sp, modifier: Modifier = Modifier) {
    val color = when (tone) {
        Tone.Warning -> ApgoPalette.warning
        Tone.Danger -> ApgoPalette.danger
        Tone.Success -> ApgoPalette.success
        Tone.Muted -> ApgoPalette.muted
    }
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (tone == Tone.Warning || tone == Tone.Danger) Icon(ApgoIcons.Warning, contentDescription = null, tint = color, modifier = Modifier.size(size.value.dp + 2.dp))
        Text(text, color = color, fontSize = size)
    }
}

/** An icon button that is lit in [tint] when [active] and dim otherwise (favorite star, ban sign). */
@Composable
fun MarkToggle(icon: ImageVector, description: String, active: Boolean, tint: Color, onClick: () -> Unit) {
    IconButton(onClick = onClick) {
        Icon(icon, contentDescription = description, tint = if (active) tint else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.45f), modifier = Modifier.size(22.dp))
    }
}

/** The content of a button: an optional icon in front of the label. */
@Composable
fun IconLabel(text: String, icon: ImageVector?, textSize: TextUnit = 12.sp) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (icon != null) Icon(icon, contentDescription = null, modifier = Modifier.size(16.dp))
        Text(text, fontSize = textSize)
    }
}
