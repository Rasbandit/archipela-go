package dev.apgo2

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.offset
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
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
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.GameInfo
import uniffi.apgo_ffi.GoalLineOut
import uniffi.apgo_ffi.HudOut
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.ZoneOut
import kotlin.math.roundToInt

// Quests shown in the Progress section before "Show all".
private const val PROGRESS_ROWS = 3
private const val LIVE_REDRAW_MS = 60_000L
private const val HIDDEN = "hidden"
private const val LOG_LINES = 3

// The shown panel's scrolling part takes this share of the space under the header; the map takes the rest.
private const val BODY_SHARE = 0.4f

// The grip is drawn this thin but can be grabbed over this height (overlapping the map and the panel), and a drag past
// SNAP_DP flips the panel.
private const val GRIP_DP = 12
private const val GRIP_ABOVE_DP = 16
private const val GRIP_BELOW_DP = 36

// The bar is centred in the touch area, which reaches further down than up: this lifts it back onto the thin strip.
private val GripLift = ((GRIP_ABOVE_DP - GRIP_BELOW_DP) / 2).dp
private const val SNAP_DP = 24
private const val GRIP_ALPHA = 0.4f
private const val SHEET_SHADOW_DP = 6
private val SheetShape = RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp)
private const val QUEST_BUBBLE_DP = 200

/**
 * The Play tab: the open game with its map, goals and progress; without one, the saved games. It stays composed under the other
 * tabs so the map comes back as it was; [onShow] is false meanwhile, and then nothing in it runs.
 */
@Composable
internal fun PlayScreen(
    m: AppModel,
    modifier: Modifier = Modifier,
    onShow: Boolean = true,
) {
    val hud = held(onShow) { m.hud }
    // Another game gets a view of its own: its map frames it afresh instead of keeping the last game's camera.
    val gameId = held(onShow) { m.engine.openGameId() }
    // Time away moves with the clock, but nothing ticks in the core: while it runs and this screen is on show, redraw once a minute
    // (display only: no GPS, and nothing while the app is in the background).
    val owner = LocalLifecycleOwner.current
    val awayRunning = hud?.awayRunning == true && onShow
    LaunchedEffect(owner, awayRunning) {
        if (awayRunning) {
            owner.repeatOnLifecycle(Lifecycle.State.STARTED) {
                while (true) {
                    delay(LIVE_REDRAW_MS)
                    m.refreshPlay(withTrace = false)
                }
            }
        }
    }
    when {
        hud != null -> key(gameId) { GameView(m, hud, onShow, modifier) }
        onShow -> NoGameOpen(m, modifier)
    }
}

// Hidden, the Play screen keeps what it last showed and reads nothing new, so GPS fixes and data changes cost it no redraw. On
// show again it reads the latest.
@Composable
private fun <T> held(
    onShow: Boolean,
    read: () -> T,
): T {
    val last = remember { Held(read()) }
    if (onShow) last.value = read()
    return last.value
}

private class Held<T>(
    var value: T,
)

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
    onShow: Boolean,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        GameHeader(m, hud)
        // The map is the top of the screen. Under it the panel is shown (goal, summary, then progress in a scrolling part) or
        // hidden (only the goal and summary stay); its grip snaps between the two.
        BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
            val bodyHeight = maxHeight * BODY_SHARE
            var shown by rememberSaveable { mutableStateOf(true) }
            var topPx by remember { mutableIntStateOf(0) }
            // How open the panel's lower part is: it follows the finger during a drag and eases to 1 or 0 on a tap or a release.
            val open = remember { Animatable(if (shown) 1f else 0f) }
            // How much of the map the panel covers right now. The map follows it step by step (during a drag too), keeping the
            // middle of what you see in the middle of what stays visible. It changes every frame of a slide, so it is read only
            // where it is used (the map's padding, the cards' layout), never while composing.
            val density = LocalDensity.current.density
            val coverDp =
                remember(bodyHeight, density) { { GRIP_DP + (topPx / density).toInt() + (bodyHeight.value * open.value).toInt() } }
            // The map fills the whole area and never resizes; the panel slides over its bottom (and is left out while hidden).
            PlayMap(m, hud, onShow, coverDp, Modifier.fillMaxSize())
            if (onShow) PlaySheet(m, hud, bodyHeight, open, shown, { shown = it }, { topPx = it }, Modifier.align(Alignment.BottomCenter))
        }
    }
}

// The panel over the map's bottom: the grip, the part that always shows, and the lower part that opens as far as the panel is
// open. That follows the finger during a drag and eases to shown or hidden on a tap or a release.
@Composable
private fun PlaySheet(
    m: AppModel,
    hud: HudOut,
    bodyHeight: Dp,
    open: Animatable<Float, AnimationVector1D>,
    shown: Boolean,
    onShowChange: (Boolean) -> Unit,
    onTopHeight: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current
    val slide = tween<Float>(OVERLAY_EASE_MS, easing = OverlayEasing)
    LaunchedEffect(shown) { open.animateTo(if (shown) 1f else 0f, slide) }
    val scope = rememberCoroutineScope()
    val bodyPx = with(density) { bodyHeight.toPx() }
    val snapPx = with(density) { SNAP_DP.dp.toPx() }
    Column(
        modifier
            .fillMaxWidth()
            .shadow(SHEET_SHADOW_DP.dp, SheetShape, clip = false) // no clip: the grip's touch area reaches up over the map
            .background(MaterialTheme.colorScheme.surface, SheetShape)
            // A hit target as a whole (as a Material Surface is): a touch anywhere on the sheet, even on plain text, never reaches
            // the map under it. The controls inside still get their touches first.
            .pointerInput(Unit) {},
    ) {
        PaneGrip(
            shown,
            onTap = { onShowChange(!shown) },
            onMove = { dy -> scope.launch { open.snapTo(PaneMode.openAfterMove(open.value, dy, bodyPx)) } },
            onRelease = { dragged ->
                // Past the snap distance it flips, otherwise it goes back; either way it settles with the map's pan.
                val target = PaneMode.afterDrag(shown, dragged, snapPx)
                onShowChange(target)
                scope.launch { open.animateTo(if (target) 1f else 0f, slide) }
            },
        )
        PanelTop(hud, shown, Modifier.onSizeChanged { onTopHeight(it.height) })
        val bodyOn by remember { derivedStateOf { open.value > 0f } }
        if (bodyOn) PanelBody(m, hud, bodyHeight) { open.value }
    }
}

// The lower part of the panel, [open] of [height] tall: its content keeps its full height, pinned to the top and cut off below.
// [open] is read when laying out, so a slide resizes it without recomposing.
@Composable
private fun PanelBody(
    m: AppModel,
    hud: HudOut,
    height: Dp,
    open: () -> Float,
) {
    val cut =
        Modifier.layout { measurable, constraints ->
            val h = (height.toPx() * open()).roundToInt()
            val p = measurable.measure(constraints.copy(minHeight = h, maxHeight = h))
            layout(p.width, h) { p.place(0, 0) }
        }
    Box(Modifier.fillMaxWidth().then(cut).clipToBounds()) {
        GamePanel(
            m,
            hud,
            Modifier
                .fillMaxWidth()
                .wrapContentHeight(Alignment.Top, unbounded = true)
                .height(height)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 8.dp),
        )
    }
}

// The grip between the map and the panel: thin to look at, easy to grab (more of its touch area lies below it, over the panel).
// A tap shows or hides the panel; a drag moves it with the finger ([onMove] gets each step, [onRelease] the whole vertical drag).
@Composable
private fun PaneGrip(
    shown: Boolean,
    onTap: () -> Unit,
    onMove: (Float) -> Unit,
    onRelease: (Float) -> Unit,
) {
    var dragged by remember { mutableFloatStateOf(0f) }
    // A drag in any direction is claimed (so a sideways swipe is never taken for a tap); only its vertical part counts.
    val drag =
        Modifier.pointerInput(Unit) {
            detectDragGestures(
                onDragStart = { dragged = 0f },
                onDragEnd = { onRelease(dragged) },
                onDragCancel = { onRelease(0f) },
            ) { change, amount ->
                change.consume()
                dragged += amount.y
                onMove(amount.y)
            }
        }
    Box(
        Modifier
            .fillMaxWidth()
            .zIndex(1f) // above the map and the panel, which its touch area overlaps
            .overhang(GRIP_DP.dp, above = GRIP_ABOVE_DP.dp, below = GRIP_BELOW_DP.dp)
            .then(drag)
            .clickable(onClickLabel = if (shown) "Hide the panel" else "Show the panel", onClick = onTap),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .offset(y = GripLift)
                .size(32.dp, 4.dp)
                .background(MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = GRIP_ALPHA), CircleShape),
        )
    }
}

// Room below for [coverDp] (the panel, read when laying out): a card at the bottom of the map sits just above the panel, and
// follows it as it slides without recomposing.
private fun Modifier.above(coverDp: () -> Int) =
    layout { measurable, constraints ->
        val lift = coverDp().dp.roundToPx()
        val p = measurable.measure(constraints.offset(vertical = -lift))
        layout(p.width, p.height + lift) { p.place(0, 0) }
    }

// Takes [thin] of the layout but reaches [above] higher and [below] lower for what follows (drawing and touches).
private fun Modifier.overhang(
    thin: Dp,
    above: Dp,
    below: Dp,
) = layout { measurable, constraints ->
    val t = thin.roundToPx()
    val up = above.roundToPx()
    val touch = up + t + below.roundToPx()
    val p = measurable.measure(constraints.copy(minHeight = touch, maxHeight = touch))
    layout(p.width, t) { p.place(0, -up) }
}

// The part of the panel that never hides: the goal (its lines too when the panel is shown) and the summary line.
@Composable
private fun PanelTop(
    hud: HudOut,
    shown: Boolean,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        GoalsBlock(hud, withLines = shown)
        Text(summaryLine(hud), fontSize = 11.sp)
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
    onShow: Boolean,
    coverDp: () -> Int,
    modifier: Modifier = Modifier,
) {
    val quests = held(onShow) { m.quests }
    val realms = held(onShow) { m.realms.filter { r -> m.zones.any { it.realmId == r.id } } }
    val me = held(onShow) { m.me }
    val trace = held(onShow) { m.trace }
    val chain = held(onShow) { m.chains.firstOrNull { it.id == m.selectedChain } }
    val selected = quests.firstOrNull { it.locationId == m.selected }
    // Selecting a quest brings its pin into view together with its popup, whose real height is measured once it is shown.
    val density = LocalDensity.current.density
    var bubblePx by remember { mutableIntStateOf(0) }
    var focus by remember { mutableStateOf<MapFocus?>(null) }
    var focusNonce by remember { mutableIntStateOf(0) }
    var anchorPx by remember { mutableStateOf<Offset?>(null) }
    // Where a trail or park was touched: its details show there instead of at its start (only while that quest stays selected).
    var touched by remember { mutableStateOf<Pair<Long, LatLng>?>(null) }
    val spot = touched?.takeIf { it.first == m.selected }?.second
    val at = spot ?: selected?.anchor?.let { LatLng(it.lat, it.lon) }
    val pinPx = if (spot != null) 0f else MapMarkers.selectedQuestPinHeightPx()
    // Forget the spot once its quest is closed or another is picked, so picking it again (from the list) opens at its start.
    LaunchedEffect(m.selected) { if (touched?.first != m.selected) touched = null }
    LaunchedEffect(m.selected, spot, bubblePx) {
        val a = at ?: return@LaunchedEffect
        val room = (if (bubblePx > 0) bubblePx else (QUEST_BUBBLE_DP * density).toInt()) + BubblePlacement.gapPx(pinPx, density)
        focus = MapFocus(a, ++focusNonce, room)
    }
    Box(modifier) {
        QuestMap(
            quests,
            realms,
            emptyList(),
            me,
            hud.thaw?.let { LatLng(it.lat, it.lon) },
            hud.waypoint?.let { LatLng(it.lat, it.lon) },
            m.selected,
            { m.selected = null }, // a tap on no pin, trail or park closes the popup
            Modifier.fillMaxSize(),
            // With a popup open, a tap inside a park (not on its outline) closes it, so there is always somewhere to tap away.
            parkAt = { at -> if (m.selected != null) null else m.engine.parkAt(at.latitude, at.longitude, m.now()) },
            onQuestClick = { id, spotAt ->
                touched = spotAt?.let { id to it }
                m.selected = id
            },
            home = m.home?.let { LatLng(it.lat, it.lon) },
            trace = trace,
            lastPlace = m.lastPlace,
            onShow = onShow,
            overlayBottomDp = coverDp,
            overlaysFollowed = true,
            focus = focus,
            anchor = at,
            onAnchor = { anchorPx = it },
        )
        selected?.let { q -> QuestPopup(m, q, anchorPx, pinPx, coverDp) { bubblePx = it } }
        chain?.let { c ->
            MapOverlayCard(Modifier.align(Alignment.BottomCenter).above(coverDp)) { ChainDetails(c) { m.selectedChain = null } }
        }
    }
}

// A quest with a pin gets a callout on it; one with no spot on the map (steps, squares, time away) gets the same card just above
// the panel.
@Composable
private fun BoxScope.QuestPopup(
    m: AppModel,
    q: QuestOut,
    anchorPx: Offset?,
    pinPx: Float,
    coverDp: () -> Int,
    onBubbleSize: (Int) -> Unit,
) {
    val details: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit = {
        QuestDetails(q) { m.selected = null }
    }
    if (q.anchor == null) {
        MapOverlayCard(Modifier.align(Alignment.BottomCenter).above(coverDp), content = details)
    } else if (anchorPx != null) {
        MapBubble(
            anchorPx,
            pinPx,
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
private fun GoalsBlock(
    hud: HudOut,
    withLines: Boolean,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (hud.goals.size > 1) {
            Text(hud.goalLabel.substringBefore(":"), style = MaterialTheme.typography.titleSmall)
            LinearProgressIndicator(progress = { hud.goalProgress }, Modifier.fillMaxWidth())
            if (withLines) hud.goals.forEach { GoalLine(it) }
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
