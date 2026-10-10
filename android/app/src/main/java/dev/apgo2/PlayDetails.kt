package dev.apgo2

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.ChainBar
import dev.apgo2.ui.ChainFormat
import dev.apgo2.ui.CollectFormat
import dev.apgo2.ui.FeedbackText
import dev.apgo2.ui.Tone
import uniffi.apgo_ffi.ChainOut
import uniffi.apgo_ffi.CollectOut
import uniffi.apgo_ffi.QuestOut

private const val PERCENT = 100

// A quest you complete by accumulating something (steps, new squares, minutes away, ground covered): name, rule and a thin
// progress bar on two lines.
@Composable
internal fun ProgressRow(
    q: QuestOut,
    onClick: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = 1.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(
                ApgoIcons.forKind(q.kindId, q.family),
                contentDescription = null,
                tint = ApgoPalette.quest(q.state),
                modifier = Modifier.size(16.dp),
            )
            Text(q.name, fontSize = 13.sp, maxLines = 1)
            Text(
                q.collect?.let(CollectFormat::row) ?: q.detail,
                fontSize = 10.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text("${(q.progress * PERCENT).toInt()}%", fontSize = 11.sp)
        }
        LinearProgressIndicator(progress = { q.progress }, Modifier.fillMaxWidth())
    }
}

// What a quest asks of you and what it pays: the content of the Play popup (a callout on the pin, or a card for quests with no pin).
@Composable
internal fun ColumnScope.QuestDetails(
    q: QuestOut,
    onClose: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(
            ApgoIcons.forKind(q.kindId, q.family),
            contentDescription = null,
            tint = ApgoPalette.kind(q.kindId, q.family),
            modifier = Modifier.size(24.dp),
        )
        Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
            Text(
                "${q.name}${if (q.boss) "  (BOSS)" else ""}",
                style = MaterialTheme.typography.titleSmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                "${q.place} · ${q.difficulty} · ~${q.effortMin.toInt()} min · ${q.mode}${if (q.fallback) " · fallback" else ""}",
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        IconButton(onClick = onClose) { Icon(ApgoIcons.Close, contentDescription = "Close") }
    }
    Text(q.detail, fontSize = 12.sp, modifier = Modifier.padding(end = 8.dp))
    q.collect?.let { CollectItems(it) }
    if (q.state == "progress") LinearProgressIndicator(progress = { q.progress }, Modifier.fillMaxWidth().padding(end = 8.dp))
    Text(q.blurb, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(end = 8.dp))
    q.reward?.let { FeedbackText("Reward: $it", Tone.Success) }
}

// One progressive quest: name and rule, a bar with a mark per check, and what is next. Tap for the list of marks.
@Composable
internal fun ChainRow(
    c: ChainOut,
    onClick: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = 2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(
                ApgoIcons.forKind(c.kindId, c.family),
                contentDescription = null,
                tint = ApgoPalette.family(c.family),
                modifier = Modifier.size(16.dp),
            )
            Text(c.name, fontSize = 13.sp, maxLines = 1)
            Text(
                c.rule,
                fontSize = 10.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
        }
        val marks = c.marks.map { it.at }
        val ticks = ChainFormat.fractions(marks, c.total)
        ChainBar(ChainFormat.fill(c.counter, c.total, marks, ticks), ticks, c.marks.map { it.reached })
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            if (ChainFormat.done(c)) {
                Icon(ApgoIcons.Check, contentDescription = "Done", tint = ApgoPalette.quest("done"), modifier = Modifier.size(12.dp))
            }
            Text(ChainFormat.next(c), fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

// The popup for a progressive quest: every mark with its amount and reward, then what is next.
@Composable
internal fun ColumnScope.ChainDetails(
    c: ChainOut,
    onClose: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(
            ApgoIcons.forKind(c.kindId, c.family),
            contentDescription = null,
            tint = ApgoPalette.family(c.family),
            modifier = Modifier.size(24.dp),
        )
        Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
            Text(c.name, style = MaterialTheme.typography.titleSmall)
            Text(c.rule, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        IconButton(onClick = onClose) { Icon(ApgoIcons.Close, contentDescription = "Close") }
    }
    c.marks.forEachIndexed { i, mk ->
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(
                if (mk.reached) ApgoIcons.Check else ApgoIcons.Locked,
                contentDescription = null,
                tint = if (mk.reached) ApgoPalette.questDone else ApgoPalette.muted,
                modifier = Modifier.size(14.dp),
            )
            Text("${i + 1}.  ${ChainFormat.amount(c.unit, mk.at)}", fontSize = 12.sp, modifier = Modifier.weight(1f))
            mk.reward?.let { Text(it, fontSize = 11.sp, color = ApgoPalette.success) }
        }
    }
    Text(ChainFormat.next(c), fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

// A forager quest's items: a check for each one picked up, its theme icon for each one still out there, and the banked total.
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CollectItems(c: CollectOut) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(CollectFormat.banked(c), fontSize = 12.sp)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            c.items.forEachIndexed { i, item ->
                Icon(
                    if (item.picked) ApgoIcons.Check else ApgoIcons.collectible(c.theme),
                    contentDescription = CollectFormat.item(c, i),
                    tint = if (item.picked) ApgoPalette.questDone else ApgoPalette.family("courier"),
                    modifier = Modifier.size(16.dp),
                )
            }
        }
    }
}
