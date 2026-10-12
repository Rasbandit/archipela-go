package dev.apgo2

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.CardFormat
import dev.apgo2.ui.FeedbackText
import dev.apgo2.ui.Tone
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.apgo_ffi.AboutOut
import uniffi.apgo_ffi.Engine
import uniffi.apgo_ffi.PlaceOut
import uniffi.apgo_ffi.ProgressOut
import uniffi.apgo_ffi.QuestOut

private const val HALF_TURN = 180f

// The Play popup: what to do first (headline), then the quest's name, its difficulty, time and mode, the measurements, the place,
// a labelled progress block, and the place's about sheet folded away. Everything it says comes from the core.
@Composable
internal fun ColumnScope.QuestCard(
    engine: Engine,
    q: QuestOut,
    onClose: () -> Unit,
) {
    Box(Modifier.fillMaxWidth()) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CardHead(q)
            MetaRow(q)
            if (q.cardDetail.isNotBlank()) {
                Text(
                    q.cardDetail,
                    fontSize = 13.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            q.placeRow?.let { PlaceRow(it) }
            q.progressBlock?.let { ProgressBlock(it, ApgoPalette.questOnCard(q.state, isSystemInDarkTheme())) }
            if (q.hasAbout) AboutFold(engine, q.locationId)
            q.reward?.let { FeedbackText("Reward: $it", Tone.Success) }
        }
        // A 48dp touch target, pulled into the corner so the icon sits tight top-right.
        IconButton(onClick = onClose, modifier = Modifier.align(Alignment.TopEnd).offset(x = 12.dp, y = (-12).dp)) {
            Icon(ApgoIcons.Close, contentDescription = "Close", modifier = Modifier.size(18.dp))
        }
    }
}

// Badge in the quest's state colour, then the goal on one line over the quest's name, centred on the badge.
@Composable
private fun CardHead(q: QuestOut) {
    Row(
        Modifier.padding(end = 28.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            Modifier.size(40.dp).background(ApgoPalette.quest(q.state), RoundedCornerShape(11.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                if (q.state == "done") ApgoIcons.Check else ApgoIcons.forKind(q.kindId, q.family),
                contentDescription = null,
                tint = ApgoPalette.badgeInk(q.state),
                modifier = Modifier.size(22.dp),
            )
        }
        Column {
            Text(
                q.headline.ifBlank { q.name },
                fontSize = 19.sp,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                "${q.name}${if (q.boss) " · boss" else ""}",
                fontSize = 14.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

// Difficulty pill, expected time and travel mode, full width under the badge.
@Composable
private fun MetaRow(q: QuestOut) {
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    val tint = ApgoPalette.difficulty(q.difficulty, isSystemInDarkTheme())
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        if (q.difficulty.isNotBlank()) {
            Text(
                q.difficulty,
                fontSize = 12.sp,
                fontWeight = FontWeight.Medium,
                color = tint,
                modifier = Modifier.border(1.dp, tint, CircleShape).padding(horizontal = 7.dp),
            )
        }
        IconText(ApgoIcons.Clock, CardFormat.effort(q.effortMin), muted)
        IconText(ApgoIcons.mode(q.mode), q.mode, muted)
    }
}

@Composable
private fun IconText(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    text: String,
    color: Color,
) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
        Icon(icon, contentDescription = null, tint = color, modifier = Modifier.size(13.dp))
        Text(text, fontSize = 12.sp, color = color)
    }
}

// "Allow Me" · artwork: the name in bold, then what the place is.
@Composable
private fun PlaceRow(p: PlaceOut) {
    val parts = CardFormat.place(p)
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Text(
        buildAnnotatedString {
            parts.name?.let { withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(it) } }
            parts.kind?.let { withStyle(SpanStyle(color = muted)) { append(it) } }
        },
        fontSize = 14.sp,
    )
}

// A labelled progress bar ("Time here 1:12 of 3:00"), or the two named steps of a courier or round trip.
@Composable
private fun ProgressBlock(
    p: ProgressOut,
    color: Color,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (p.steps.isEmpty()) ProgressBar(p, color) else Steps(p.steps, p.stepAt.toInt(), color)
        p.hint?.let { Text(it, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

@Composable
private fun ProgressBar(
    p: ProgressOut,
    color: Color,
) {
    // Read as one item: "Time here, 1:12 of 3:00", then the bar's percentage.
    Column(Modifier.semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.fillMaxWidth()) {
            Text(p.label, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f))
            Text(p.value, fontSize = 12.sp, fontWeight = FontWeight.Medium)
        }
        LinearProgressIndicator(progress = { p.frac.coerceIn(0f, 1f) }, Modifier.fillMaxWidth(), color = color)
    }
}

@Composable
private fun Steps(
    steps: List<String>,
    at: Int,
    color: Color,
) {
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        steps.forEachIndexed { i, label ->
            if (i > 0) Box(Modifier.width(18.dp).height(2.dp).background(if (at >= i) color else muted))
            StepDot(i, at, color)
            Text(label, fontSize = 12.sp, fontWeight = if (at == i) FontWeight.Medium else FontWeight.Normal)
        }
    }
}

@Composable
private fun StepDot(
    i: Int,
    at: Int,
    color: Color,
) {
    val done = at > i
    Box(
        Modifier.size(18.dp).background(if (done || at == i) color else MaterialTheme.colorScheme.surfaceVariant, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        if (done) {
            Icon(ApgoIcons.Check, contentDescription = "Done", tint = ApgoPalette.inkOn(color), modifier = Modifier.size(12.dp))
        } else {
            Text(
                "${i + 1}",
                fontSize = 10.sp,
                color =
                    if (at ==
                        i
                    ) {
                        ApgoPalette.inkOn(color)
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
            )
        }
    }
}

// "About this place": folded until tapped, then read from the realm's scan off the UI thread.
@Composable
private fun AboutFold(
    engine: Engine,
    locationId: Long,
) {
    var open by rememberSaveable(locationId) { mutableStateOf(false) }
    var about by remember(locationId) { mutableStateOf<AboutOut?>(null) }
    if (open) {
        LaunchedEffect(locationId) {
            about = withContext(Dispatchers.IO) { runCatching { engine.about(locationId) }.getOrNull() }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clickable(role = Role.Button, onClickLabel = if (open) "Hide details" else "Show details") { open = !open }
                .semantics { stateDescription = if (open) "Expanded" else "Collapsed" },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("About this place", fontSize = 13.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
            Icon(
                ApgoIcons.Expand,
                contentDescription = null,
                modifier = Modifier.size(16.dp).rotate(if (open) HALF_TURN else 0f),
            )
        }
        if (open) about?.let { AboutSheet(it) }
    }
}

@Composable
private fun AboutSheet(a: AboutOut) {
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        a.description?.let { Text(it, fontSize = 12.sp, color = muted) }
        a.rows.forEach { r ->
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(r.label, fontSize = 12.sp, color = muted, modifier = Modifier.width(76.dp))
                Text(r.value, fontSize = 12.sp, modifier = Modifier.weight(1f))
            }
        }
        a.wikipediaUrl?.let { url ->
            // A real link: TalkBack announces it as one, and the platform opens it.
            Text(
                buildAnnotatedString {
                    withLink(LinkAnnotation.Url(url, TextLinkStyles(SpanStyle(color = MaterialTheme.colorScheme.primary)))) {
                        append("Read more on Wikipedia ↗")
                    }
                },
                fontSize = 12.sp,
                modifier = Modifier.heightIn(min = 48.dp).wrapContentHeight(),
            )
        }
    }
}
