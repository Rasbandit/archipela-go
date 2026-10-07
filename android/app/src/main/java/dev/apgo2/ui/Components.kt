package dev.apgo2.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
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
fun ApgoChip(label: String, selected: Boolean, onClick: () -> Unit, textSize: TextUnit = 12.sp) {
    FilterChip(
        selected = selected,
        onClick = onClick,
        label = { Text(label, fontSize = textSize) },
        colors = FilterChipDefaults.filterChipColors(
            selectedContainerColor = MaterialTheme.colorScheme.primary,
            selectedLabelColor = MaterialTheme.colorScheme.onPrimary,
        ),
    )
}

/** One row of single-choice chips. */
@Composable
fun <T> ChoiceChips(options: List<T>, selected: T, onSelect: (T) -> Unit, label: (T) -> String, textSize: TextUnit = 12.sp, spacing: Dp = 6.dp) {
    Row(horizontalArrangement = Arrangement.spacedBy(spacing)) {
        options.forEach { ApgoChip(label(it), it == selected, { onSelect(it) }, textSize) }
    }
}

@Composable
fun ModeChips(selected: String, onSelect: (String) -> Unit) = ChoiceChips(MODES, selected, onSelect, ::modeLabel)

/** A panel floating over a full-page map. */
@Composable
fun MapOverlayCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Card(modifier.fillMaxWidth().padding(8.dp)) {
        Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp), content = content)
    }
}

enum class Tone { Warning, Danger, Success, Muted }

@Composable
fun FeedbackText(text: String, tone: Tone, size: TextUnit = 12.sp, modifier: Modifier = Modifier) {
    val color = when (tone) {
        Tone.Warning -> ApgoPalette.warning
        Tone.Danger -> ApgoPalette.danger
        Tone.Success -> ApgoPalette.success
        Tone.Muted -> ApgoPalette.muted
    }
    Text(text, modifier, color = color, fontSize = size)
}

/** A glyph button that is lit in [tint] when [active] and dim otherwise (favorite star, ban sign). */
@Composable
fun MarkToggle(glyph: String, active: Boolean, tint: Color, onClick: () -> Unit) {
    IconButton(onClick = onClick) {
        Text(glyph, fontSize = 20.sp, color = if (active) tint else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.45f))
    }
}
