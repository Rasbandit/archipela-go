package dev.apgo2

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.presence.PresenceText
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.BubblePlacement
import dev.apgo2.ui.FeedbackText
import dev.apgo2.ui.METERS_PER_KM
import dev.apgo2.ui.MapBubble
import dev.apgo2.ui.MapMarkers
import dev.apgo2.ui.MapOverlayCard
import dev.apgo2.ui.Tone
import dev.apgo2.ui.Units
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.GameInfo
import uniffi.apgo_ffi.GoalLineOut
import uniffi.apgo_ffi.HudOut
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.ZoneOut

// Quests shown in the Progress section before "Show all".
private const val PROGRESS_ROWS = 3
private const val HIDDEN = "hidden"
private const val LOG_LINES = 3
private const val MAP_WEIGHT = 0.55f
private const val PANEL_WEIGHT = 0.45f
private const val QUEST_BUBBLE_DP = 200

/** The Play tab: the open game with its map, goals and progress; without one, the saved games. */
@Composable
internal fun PlayScreen(
    m: AppModel,
    modifier: Modifier = Modifier,
) {
    val hud = m.hud
    if (hud == null) NoGameOpen(m, modifier) else GameView(m, hud, modifier)
}

@Composable
private fun NoGameOpen(
    m: AppModel,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text("Play", style = MaterialTheme.typography.titleLarge)
        Text("No game is open, so nothing is being tracked.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        GamesList(m)
        OutlinedButton(onClick = { m.tab = AppTab.NEW_GAME }) { Text("New game") }
    }
}

@Composable
private fun GameView(
    m: AppModel,
    hud: HudOut,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        GameHeader(m, hud)
        // The map is the top of the screen; goals and progress-bar quests sit under it in a scrolling panel.
        PlayMap(m, hud, Modifier.fillMaxWidth().weight(MAP_WEIGHT))
        GamePanel(
            m,
            hud,
            Modifier
                .fillMaxWidth()
                .weight(PANEL_WEIGHT)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 8.dp),
        )
    }
}

@Composable
private fun GameHeader(
    m: AppModel,
    hud: HudOut,
) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column {
            Text("${hud.gameName}  ·  ${hud.backend}", fontSize = 12.sp)
            Text(
                PresenceText.chip(m.presence.decision.state, m.settings.homeNetworks.isNotEmpty() || m.settings.carDevices.isNotEmpty()),
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.primary,
            )
        }
        OutlinedButton(onClick = { m.library.pause() }) {
            Icon(ApgoIcons.Pause, contentDescription = null, modifier = Modifier.size(16.dp))
            Text(" Stop playing", fontSize = 12.sp)
        }
    }
}

// The map with the quests, and the popup of the selected quest or progressive quest.
@Composable
private fun PlayMap(
    m: AppModel,
    hud: HudOut,
    modifier: Modifier = Modifier,
) {
    val selected = m.quests.firstOrNull { it.locationId == m.selected }
    // Selecting a quest brings its pin into view together with its popup, whose real height is measured once it is shown.
    val density = LocalDensity.current.density
    var bubblePx by remember { mutableIntStateOf(0) }
    var focus by remember { mutableStateOf<MapFocus?>(null) }
    var focusNonce by remember { mutableIntStateOf(0) }
    var anchorPx by remember { mutableStateOf<Offset?>(null) }
    LaunchedEffect(m.selected, bubblePx) {
        val a = selected?.anchor ?: return@LaunchedEffect
        val pin = MapMarkers.selectedQuestPinHeightPx(selected.difficulty, selected.boss)
        val room = (if (bubblePx > 0) bubblePx else (QUEST_BUBBLE_DP * density).toInt()) + BubblePlacement.gapPx(pin, density)
        focus = MapFocus(LatLng(a.lat, a.lon), ++focusNonce, room)
    }
    Box(modifier) {
        QuestMap(
            m.quests,
            m.realms.filter { r -> m.zones.any { it.realmId == r.id } },
            emptyList(),
            m.me,
            hud.thaw?.let { LatLng(it.lat, it.lon) },
            hud.waypoint?.let { LatLng(it.lat, it.lon) },
            m.selected,
            { ll -> nearestQuest(m.quests, ll)?.let { m.selected = it.locationId } },
            Modifier.fillMaxSize(),
            onQuestClick = { m.selected = it },
            home = m.home?.let { LatLng(it.lat, it.lon) },
            trace = m.trace,
            focus = focus,
            anchor = selected?.anchor?.let { LatLng(it.lat, it.lon) },
            onAnchor = { anchorPx = it },
        )
        selected?.let { q -> QuestPopup(m, q, anchorPx) { bubblePx = it } }
        m.chains.firstOrNull { it.id == m.selectedChain }?.let { c ->
            MapOverlayCard(Modifier.align(Alignment.BottomCenter)) { ChainDetails(c) { m.selectedChain = null } }
        }
    }
}

// The quest whose pin is nearest to a tap on the map.
private fun nearestQuest(
    quests: List<QuestOut>,
    tap: LatLng,
): QuestOut? =
    quests
        .filter { it.anchor != null && it.state != HIDDEN }
        .minByOrNull { q ->
            val a = q.anchor
            val d = floatArrayOf(0f)
            if (a != null) android.location.Location.distanceBetween(tap.latitude, tap.longitude, a.lat, a.lon, d)
            d[0]
        }

// A quest with a pin gets a callout on it; one with no spot on the map (steps, squares, time away) gets the same card at the bottom.
@Composable
private fun BoxScope.QuestPopup(
    m: AppModel,
    q: QuestOut,
    anchorPx: Offset?,
    onBubbleSize: (Int) -> Unit,
) {
    val details: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit = {
        QuestDetails(q) { m.selected = null }
    }
    if (q.anchor == null) {
        MapOverlayCard(Modifier.align(Alignment.BottomCenter), content = details)
    } else if (anchorPx != null) {
        MapBubble(
            anchorPx,
            MapMarkers.selectedQuestPinHeightPx(q.difficulty, q.boss),
            onSize = { onBubbleSize(it.height) },
            content = details,
        )
    }
}

// Progress, goals, the numbers so far, zones, traps, the list of places and the latest messages.
@Composable
private fun GamePanel(
    m: AppModel,
    hud: HudOut,
    modifier: Modifier = Modifier,
) {
    val layout = remember(m.quests) { PlayLayout.split(m.quests) }
    var showPlaces by remember { mutableStateOf(false) }
    var allProgress by remember { mutableStateOf(false) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (m.chains.isNotEmpty() || layout.progress.isNotEmpty()) {
            ProgressSection(m, layout, allProgress) { allProgress = !allProgress }
        }
        GoalsBlock(hud)
        Text(summaryLine(hud), fontSize = 11.sp)
        ZonesRow(m.zones)
        (hud.traps + listOfNotNull(hud.blocked)).distinct().takeIf { it.isNotEmpty() }?.let {
            FeedbackText(it.joinToString("  ·  "), Tone.Danger)
        }
        TextButton(onClick = { showPlaces = !showPlaces }) {
            Text("${if (showPlaces) "Hide" else "Show"} places on the map (${layout.places.size})", fontSize = 12.sp)
        }
        if (showPlaces && layout.places.isNotEmpty()) PlacesList(m, layout.places)
        if (m.log.isNotEmpty()) Text(m.log.take(LOG_LINES).joinToString("\n"), fontSize = 11.sp, color = MaterialTheme.colorScheme.primary)
    }
}

@Composable
private fun ProgressSection(
    m: AppModel,
    layout: PlayLayout.Split,
    allProgress: Boolean,
    onToggleAll: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("Progress", style = MaterialTheme.typography.titleSmall)
        m.chains.sortedBy { c -> c.marks.all { it.reached } }.forEach { c ->
            ChainRow(c) {
                m.selectedChain = c.id
                m.selected = null
            }
        }
        (if (allProgress) layout.progress else layout.progress.take(PROGRESS_ROWS)).forEach { q ->
            ProgressRow(q) {
                m.selected = q.locationId
                m.selectedChain = null
            }
        }
        if (layout.progress.size > PROGRESS_ROWS) {
            TextButton(onClick = onToggleAll) {
                Text(if (allProgress) "Show fewer" else "Show all ${layout.progress.size}", fontSize = 11.sp)
            }
        }
    }
}

// The goal, or several goals: the rule and overall progress, then each goal with its own bar.
@Composable
private fun GoalsBlock(hud: HudOut) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (hud.goals.size > 1) {
            Text(hud.goalLabel.substringBefore(":"), style = MaterialTheme.typography.titleSmall)
            LinearProgressIndicator(progress = { hud.goalProgress }, Modifier.fillMaxWidth())
            hud.goals.forEach { GoalLine(it) }
        } else {
            Text(hud.goalLabel, style = MaterialTheme.typography.titleSmall)
            LinearProgressIndicator(progress = { hud.goalProgress }, Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun GoalLine(g: GoalLineOut) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Icon(
            if (g.achieved) ApgoIcons.Check else ApgoIcons.Play,
            contentDescription = if (g.achieved) "Done" else "Not done",
            tint = if (g.achieved) ApgoPalette.success else ApgoPalette.muted,
            modifier = Modifier.size(14.dp),
        )
        Column(Modifier.weight(1f)) {
            Text(g.label, fontSize = 11.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
            LinearProgressIndicator(progress = { g.progress }, Modifier.fillMaxWidth())
        }
    }
}

private fun summaryLine(hud: HudOut): String {
    val tools = hud.tools.joinToString().ifBlank { "no tools" }
    return "Quests ${hud.done}/${hud.total} · keys ${hud.keys} · $tools · letters ${hud.letters.ifBlank { "-" }} · " +
        "${Units.distance(hud.distanceKm * METERS_PER_KM)} · streak ${hud.streakDays}d"
}

@Composable
private fun ZonesRow(zones: List<ZoneOut>) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        zones.forEach { ZoneChip(it) }
    }
}

@Composable
private fun ZoneChip(z: ZoneOut) {
    val tint = if (z.unlocked) ApgoPalette.success else ApgoPalette.muted
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
        Icon(ApgoIcons.mode(z.mode), contentDescription = null, tint = tint, modifier = Modifier.size(13.dp))
        Text("Z${z.id}", fontSize = 11.sp, color = tint)
        Icon(
            if (z.unlocked) ApgoIcons.Unlocked else ApgoIcons.Locked,
            contentDescription = if (z.unlocked) "Unlocked" else "Locked",
            tint = tint,
            modifier = Modifier.size(13.dp),
        )
        if (!z.unlocked) Text(unlockCost(z), fontSize = 11.sp, color = tint)
    }
}

// What a locked zone still needs: its keys and its tool, like "3key+bike".
private fun unlockCost(z: ZoneOut): String {
    val keys = if (z.keysNeeded > 0u) "${z.keysNeeded}key" else ""
    return keys + (z.tool?.let { "+$it" } ?: "")
}

@Composable
private fun PlacesList(
    m: AppModel,
    places: List<QuestOut>,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        places.forEach { q ->
            Row(
                Modifier.fillMaxWidth().clickable { m.selected = q.locationId }.padding(vertical = 3.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(
                    ApgoIcons.forKind(q.kindId, q.family),
                    contentDescription = null,
                    tint = ApgoPalette.quest(q.state),
                    modifier = Modifier.size(20.dp),
                )
                Column(Modifier.weight(1f)) {
                    Text(if (q.state == HIDDEN) "??? (undiscovered)" else q.name, fontSize = 13.sp)
                    if (q.state != HIDDEN) Text("${q.place} · ${q.difficulty} · ~${q.effortMin.toInt()} min", fontSize = 10.sp)
                }
                Text(q.state, fontSize = 10.sp)
            }
        }
    }
}

// The saved games: open one to start (or resume) tracking, or delete it (its recorded data is kept for diagnosis).
@Composable
private fun GamesList(
    m: AppModel,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("Continue a game", style = MaterialTheme.typography.titleMedium)
        if (m.games.isEmpty()) Text("No saved games yet.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        m.games.forEach { g -> SavedGameRow(m, g) }
    }
}

@Composable
private fun SavedGameRow(
    m: AppModel,
    g: GameInfo,
) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
        Text(g.name)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Button(onClick = { m.library.openGame(g.id) }) { Text("Open", fontSize = 12.sp) }
            OutlinedButton(onClick = { m.library.deleteGame(g.id) }) { Text("Delete", fontSize = 12.sp) }
        }
    }
}
