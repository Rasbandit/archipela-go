package dev.apgo2.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RichTooltip
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipAnchorPosition
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.TooltipScope
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

/*
 * One way to explain things everywhere. A topic (title and body) is written once in HelpText.kt; screens only point at it.
 * Tapping the ⓘ opens the explanation next to it, and so does pressing and holding the label it belongs to. It stays open until
 * you tap elsewhere, and only one is open at a time.
 */

/** A small ⓘ that explains [topic]. Use it beside a control or heading; use [LabelWithHelp] when there is a label to press and hold. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun HelpTip(
    topic: HelpTopic,
    modifier: Modifier = Modifier,
) {
    val state = rememberTooltipState(isPersistent = true)
    val scope = rememberCoroutineScope()
    TooltipBox(
        positionProvider = TooltipDefaults.rememberTooltipPositionProvider(TooltipAnchorPosition.Above),
        tooltip = { TopicTooltip(topic) },
        state = state,
        modifier = modifier,
    ) {
        IconButton(onClick = { scope.launch { state.show() } }, modifier = Modifier.size(28.dp)) {
            Icon(
                ApgoIcons.Help,
                contentDescription = "About ${topic.title}",
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(18.dp),
            )
        }
    }
}

/** A label with its ⓘ. Pressing and holding the label (or tapping the ⓘ) opens the explanation. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LabelWithHelp(
    text: String,
    topic: HelpTopic,
    modifier: Modifier = Modifier,
    style: TextStyle = MaterialTheme.typography.bodyMedium,
) {
    val state = rememberTooltipState(isPersistent = true)
    val scope = rememberCoroutineScope()
    TooltipBox(
        positionProvider = TooltipDefaults.rememberTooltipPositionProvider(TooltipAnchorPosition.Above),
        tooltip = { TopicTooltip(topic) },
        state = state,
        modifier = modifier,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(text, style = style)
            IconButton(onClick = { scope.launch { state.show() } }, modifier = Modifier.size(28.dp)) {
                Icon(
                    ApgoIcons.Help,
                    contentDescription = "About ${topic.title}",
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TooltipScope.TopicTooltip(topic: HelpTopic) {
    RichTooltip(title = { Text(topic.title) }) { Text(topic.body) }
}
