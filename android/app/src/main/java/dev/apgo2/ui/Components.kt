package dev.apgo2.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilledIconToggleButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Shared building blocks. Screens compose these; they do not restyle Material widgets themselves. */

internal val MODES = listOf("walk", "run", "bike", "drive")

/** The modes a realm or zone can be for right now. Car is not offered yet. */
internal val PLAY_MODES = listOf("walk", "run", "bike")

/** The game says "car" where the data says "drive". */
internal fun modeLabel(mode: String) = if (mode == "drive") "car" else mode

/** A selectable chip. The selected one is a solid fill, so it reads clearly against the card behind it. */
@Composable
internal fun ApgoChip(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    textSize: TextUnit = 12.sp,
    icon: ImageVector? = null,
    enabled: Boolean = true,
) {
    FilterChip(
        modifier = modifier,
        selected = selected,
        onClick = onClick,
        enabled = enabled,
        label = { Text(label, fontSize = textSize) },
        leadingIcon = icon?.let { { Icon(it, contentDescription = null, modifier = Modifier.size(16.dp)) } },
        colors =
            FilterChipDefaults.filterChipColors(
                selectedContainerColor = MaterialTheme.colorScheme.primary,
                selectedLabelColor = MaterialTheme.colorScheme.onPrimary,
                selectedLeadingIconColor = MaterialTheme.colorScheme.onPrimary,
            ),
    )
}

/** One row of single-choice chips, optionally with an icon on each. */
@Composable
internal fun <T> ChoiceChips(
    options: List<T>,
    selected: T,
    onSelect: (T) -> Unit,
    label: (T) -> String,
    modifier: Modifier = Modifier,
    textSize: TextUnit = 12.sp,
    spacing: Dp = 6.dp,
    icon: (T) -> ImageVector? = { null },
) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(spacing)) {
        options.forEach { ApgoChip(label(it), it == selected, { onSelect(it) }, textSize = textSize, icon = icon(it)) }
    }
}

/** A panel floating over a full-page map. With [fillHeight] its content may use all the height the caller gives the card (for lists). */
@Composable
internal fun MapOverlayCard(
    modifier: Modifier = Modifier,
    fillHeight: Boolean = false,
    content: @Composable ColumnScope.() -> Unit,
) {
    Card(modifier.fillMaxWidth().padding(8.dp)) {
        Column(
            Modifier.padding(10.dp).then(if (fillHeight) Modifier.fillMaxHeight() else Modifier),
            verticalArrangement = Arrangement.spacedBy(4.dp),
            content = content,
        )
    }
}

/** Where a callout goes relative to the pin it describes. */
internal object BubblePlacement {
    private const val PIN_CLEARANCE_DP = 6

    /**
     * Top-left of the bubble in the container's pixels: above the pin and centred on it when that fits, otherwise below, and
     * always inside the container.
     */
    fun place(
        pinX: Float,
        pinY: Float,
        width: Int,
        height: Int,
        containerW: Int,
        containerH: Int,
        margin: Int,
        gap: Int,
    ): Pair<Int, Int> {
        val x = (pinX - width / 2f).toInt().coerceIn(margin, maxOf(margin, containerW - width - margin))
        val above = pinY - height - gap
        val y = if (above >= margin) above.toInt() else (pinY + gap / 2f).toInt().coerceAtMost(maxOf(margin, containerH - height - margin))
        return x to y
    }

    /** The gap between a point and a callout above it: the pin standing on the point ([pinHeightPx]) plus a little air. */
    fun gapPx(
        pinHeightPx: Float,
        density: Float,
    ): Int = (pinHeightPx + PIN_CLEARANCE_DP * density).toInt()
}

/**
 * A callout attached to a pin on a full map: [at] is the pin's point in the map's pixels and [pinHeightPx] how tall the pin stands
 * above it; [onSize] reports the callout's measured size (so the map can make room for it). Used by the realm editor (a find) and
 * the Play map (a quest).
 */
@Composable
internal fun MapBubble(
    at: androidx.compose.ui.geometry.Offset,
    pinHeightPx: Float,
    onSize: (androidx.compose.ui.unit.IntSize) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val density = androidx.compose.ui.platform.LocalDensity.current
    val margin = with(density) { 8.dp.roundToPx() }
    val gap = BubblePlacement.gapPx(pinHeightPx, density.density)
    val maxWidth = with(density) { 300.dp.roundToPx() }
    Box(
        modifier.layout { measurable, constraints ->
            val p =
                measurable.measure(
                    constraints.copy(
                        minWidth = 0,
                        minHeight = 0,
                        maxWidth =
                            minOf(
                                maxWidth,
                                constraints.maxWidth - 2 * margin,
                            ),
                    ),
                )
            layout(constraints.maxWidth, constraints.maxHeight) {
                val (x, y) = BubblePlacement.place(at.x, at.y, p.width, p.height, constraints.maxWidth, constraints.maxHeight, margin, gap)
                p.place(x, y)
            }
        },
    ) {
        Card(Modifier.onSizeChanged(onSize), elevation = CardDefaults.cardElevation(6.dp)) {
            Column(
                Modifier.padding(start = 12.dp, top = 8.dp, bottom = 10.dp, end = 4.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
                content = content,
            )
        }
    }
}

internal enum class Tone { Warning, Danger, Success, Muted }

/** A line of status text; warnings and errors get a warning icon in front. */
@Composable
internal fun FeedbackText(
    text: String,
    tone: Tone,
    modifier: Modifier = Modifier,
    size: TextUnit = 12.sp,
) {
    val color =
        when (tone) {
            Tone.Warning -> ApgoPalette.warning
            Tone.Danger -> ApgoPalette.danger
            Tone.Success -> ApgoPalette.success
            Tone.Muted -> ApgoPalette.muted
        }
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (tone == Tone.Warning ||
            tone == Tone.Danger
        ) {
            Icon(
                ApgoIcons.Warning,
                contentDescription = null,
                tint = color,
                modifier =
                    Modifier.size(size.value.dp + 2.dp),
            )
        }
        Text(text, color = color, fontSize = size)
    }
}

/** An icon button that is lit in [tint] when [active] and dim otherwise (favorite star, ban sign). */
@Composable
internal fun MarkToggle(
    icon: ImageVector,
    description: String,
    active: Boolean,
    tint: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    IconButton(onClick = onClick, modifier = modifier) {
        Icon(
            icon,
            contentDescription = description,
            tint = if (active) tint else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.45f),
            modifier = Modifier.size(22.dp),
        )
    }
}

/** The content of a button: an optional icon in front of the label. */
@Composable
internal fun IconLabel(
    text: String,
    icon: ImageVector?,
    modifier: Modifier = Modifier,
    textSize: TextUnit = 12.sp,
) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (icon != null) Icon(icon, contentDescription = null, modifier = Modifier.size(16.dp))
        Text(text, fontSize = textSize)
    }
}

/** A row of icon-only single-choice toggles, for tight spaces. Each needs a [description] for accessibility. */
@Composable
internal fun <T> IconChoices(
    options: List<T>,
    selected: T,
    onSelect: (T) -> Unit,
    icon: (T) -> ImageVector,
    description: (T) -> String,
    modifier: Modifier = Modifier,
) {
    Row(modifier) {
        options.forEach { option ->
            FilledIconToggleButton(
                checked = option == selected,
                onCheckedChange = { onSelect(option) },
                colors = toolToggleColors(),
            ) { Icon(icon(option), contentDescription = description(option), modifier = Modifier.size(20.dp)) }
        }
    }
}
