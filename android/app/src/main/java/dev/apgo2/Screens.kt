package dev.apgo2

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import org.maplibre.android.geometry.LatLng
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.SoloOptionsIn

private val MODES = listOf("walk", "run", "bike", "drive")
private val FAMILIES = listOf("reach", "dwell", "landmark", "trail", "park", "water", "courier", "explore", "steps", "away")
private val GOALS = listOf(
    Triple("macguffin_short", "Letter Hunt", "Collect the letters A-P-G-O"),
    Triple("macguffin_long", "Letter Hunt XL", "Collect ARCHIPELAGO"),
    Triple("all_trips", "Completionist", "Finish every quest"),
    Triple("boss", "The Big One", "Beat the boss quest"),
    Triple("treasure_hunt", "Treasure Hunt", "Find the letters, then claim the treasure"),
    Triple("zone_conqueror", "Zone Conqueror", "60% of every zone"),
    Triple("well_rounded", "Well Rounded", "One quest of every type"),
    Triple("quest_dex", "Quest-dex", "15 different kinds of quest"),
    Triple("marathon", "Marathon", "42 km of tracked travel"),
    Triple("explorer", "Explorer", "Reveal 300 map cells"),
    Triple("streak", "Daily Habit", "Quest 7 days in a row"),
    Triple("boss_rush", "Boss Rush", "Finish 5 hard quests"),
)
private val TRAPS = listOf("freeze", "fog", "shuffle", "silence", "leash", "detour", "toll", "slow", "honor")

private fun stateColor(s: String) = when (s) {
    "done" -> Color(0xFF2E7D32)
    "progress" -> Color(0xFFF9A825)
    "locked" -> Color(0xFF9E9E9E)
    "hidden" -> Color(0xFFBDBDBD)
    else -> Color(0xFFD32F2F)
}

@Composable
fun ModeChips(selected: String, onSelect: (String) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        MODES.forEach { m -> FilterChip(selected = selected == m, onClick = { onSelect(m) }, label = { Text(if (m == "drive") "car" else m, fontSize = 12.sp) }) }
    }
}

@Composable
fun AppRoot(m: AppModel) {
    Scaffold(
        bottomBar = {
            NavigationBar {
                listOf("Realms", "New Game", "Play").forEachIndexed { i, t ->
                    NavigationBarItem(selected = m.tab == i, onClick = { m.tab = i }, icon = { Text(listOf("◎", "✚", "▶")[i]) }, label = { Text(t) })
                }
            }
        },
    ) { pad ->
        Column(Modifier.fillMaxSize().padding(pad)) {
            m.busy?.let { Text(it, Modifier.padding(horizontal = 12.dp, vertical = 4.dp), fontSize = 12.sp); LinearProgressIndicator(Modifier.fillMaxWidth()) }
            if (m.status.isNotBlank()) Text(m.status, Modifier.padding(horizontal = 12.dp, vertical = 2.dp), fontSize = 12.sp, color = MaterialTheme.colorScheme.primary)
            Box(Modifier.weight(1f)) {
                when (m.tab) {
                    0 -> RealmsScreen(m)
                    1 -> NewGameScreen(m)
                    else -> PlayScreen(m)
                }
            }
        }
    }
    m.yamlText?.let { y ->
        val clip = LocalClipboardManager.current
        AlertDialog(
            onDismissRequest = { m.yamlText = null },
            title = { Text("Archipelago YAML") },
            text = { Text(y, fontSize = 11.sp, modifier = Modifier.verticalScroll(rememberScrollState())) },
            confirmButton = { TextButton(onClick = { clip.setText(AnnotatedString(y)); m.status = "YAML copied" ; m.yamlText = null }) { Text("Copy") } },
            dismissButton = { TextButton(onClick = { m.yamlText = null }) { Text("Close") } },
        )
    }
}

// ------------------------------------------------------------------ realms
/** Two views: the list of realms, and a full-page map editor for a new one. */
@Composable
fun RealmsScreen(m: AppModel) {
    var editing by remember { mutableStateOf(false) }
    if (editing) RealmEditor(m) { editing = false } else RealmList(m) { editing = true }
}

@Composable
private fun RealmList(m: AppModel, onNew: () -> Unit) {
    var expanded by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize().padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Text("Realms: places you play in", style = MaterialTheme.typography.titleMedium)
            Button(onClick = onNew) { Text("+ New realm", fontSize = 12.sp) }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedButton(onClick = { m.setHomeHere() }) { Text("Set home here", fontSize = 12.sp) }
            Text(if (m.home == null) "No home yet: distances are measured from your first realm." else "Home is set. Distances are measured from it.", fontSize = 11.sp)
        }
        if (m.realms.isEmpty()) Text("No realms yet. Tap + New realm to draw one.", fontSize = 13.sp)
        LazyColumn(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            items(m.realms, key = { it.id }) { r ->
                Card(Modifier.fillMaxWidth().clickable { expanded = if (expanded == r.id) null else r.id }) {
                    Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                            Text(r.name, style = MaterialTheme.typography.titleSmall)
                            Text(if (r.mode == "drive") "car" else r.mode, color = MaterialTheme.colorScheme.primary)
                        }
                        val on = m.offers[r.id].orEmpty()
                        Text(if (r.scannedAtMs == null) "Not scanned yet" else "${r.places} places · ${on.size} quest kinds on offer", fontSize = 12.sp)
                        r.warning?.let { Text("⚠ $it", fontSize = 11.sp, color = Color(0xFFE65100)) }
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            OutlinedButton(onClick = { m.scan(r.id) }) { Text(if (r.scannedAtMs == null) "Scan" else "Rescan", fontSize = 12.sp) }
                            OutlinedButton(onClick = { m.deleteRealm(r.id) }) { Text("Delete", fontSize = 12.sp) }
                        }
                        if (expanded == r.id) {
                            HorizontalDivider()
                            on.take(30).forEach { Text("${it.name}${if (it.count > 0u) "  ×${it.count}" else ""}  (${it.family})", fontSize = 12.sp) }
                            if (on.size > 30) Text("…and ${on.size - 30} more", fontSize = 11.sp)
                        }
                    }
                }
            }
        }
    }
}

/** The map is the whole page; name, mode and shape controls float over it. */
@Composable
private fun RealmEditor(m: AppModel, onClose: () -> Unit) {
    var name by remember { mutableStateOf("") }
    var mode by remember { mutableStateOf("walk") }
    var polygon by remember { mutableStateOf(false) }
    var radius by remember { mutableFloatStateOf(1500f) }
    var center by remember { mutableStateOf<LatLng?>(null) }
    val circleCenter = center ?: m.me // follows your GPS until it is moved
    DisposableEffect(Unit) { onDispose { m.draft.clear() } }

    Box(Modifier.fillMaxSize()) {
        QuestMap(
            emptyList(), m.realms, m.draft.toList(), m.me, null, null, null, { if (polygon) m.draft.add(it) },
            Modifier.fillMaxSize(),
            home = m.home?.let { LatLng(it.lat, it.lon) },
            onMapLongClick = { m.setHome(it) },
            circle = if (polygon) null else circleCenter?.let { it to radius.toDouble() },
            overlayTopDp = 150, overlayBottomDp = 230,
        )
        Card(Modifier.align(Alignment.TopCenter).fillMaxWidth().padding(8.dp)) {
            Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    TextButton(onClick = onClose) { Text("← Back") }
                    OutlinedTextField(name, { name = it }, label = { Text("Realm name") }, singleLine = true, modifier = Modifier.weight(1f))
                }
                ModeChips(mode) { mode = it }
            }
        }
        Card(Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(8.dp)) {
            Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    FilterChip(selected = !polygon, onClick = { polygon = false }, label = { Text("Circle") })
                    FilterChip(selected = polygon, onClick = { polygon = true }, label = { Text("Polygon") })
                }
                if (polygon) {
                    Text("Tap the map to add corners (${m.draft.size}); 3 or more makes a realm.", fontSize = 12.sp)
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        OutlinedButton(onClick = { if (m.draft.isNotEmpty()) m.draft.removeAt(m.draft.lastIndex) }) { Text("Undo", fontSize = 12.sp) }
                        OutlinedButton(onClick = { m.draft.clear() }) { Text("Clear", fontSize = 12.sp) }
                    }
                } else {
                    Text("Radius: ${radius.toInt()} m, centered on you.", fontSize = 12.sp)
                    Slider(radius, { radius = it }, valueRange = 300f..8000f)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Button(onClick = {
                        val ok = if (polygon) m.saveDraftRealm(name, mode) else m.saveCircleRealm(name, mode, radius.toDouble(), circleCenter)
                        if (ok) onClose()
                    }) { Text("Save + scan") }
                    OutlinedButton(onClick = onClose) { Text("Cancel") }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- new game
@Composable
fun NewGameScreen(m: AppModel) {
    var goal by remember { mutableStateOf("macguffin_short") }
    var target by remember { mutableStateOf("") }
    var trips by remember { mutableFloatStateOf(60f) }
    var preset by remember { mutableStateOf(1) }
    var mpt by remember { mutableFloatStateOf(10f) }
    var fog by remember { mutableStateOf(false) }
    var trapsOn by remember { mutableStateOf(true) }
    var bonus by remember { mutableStateOf(true) }
    var name by remember { mutableStateOf("My game") }
    val families = remember { mutableStateListOf(*FAMILIES.toTypedArray()) }
    val zoneRealms = remember { mutableStateListOf<String>() }
    var url by remember { mutableStateOf("localhost:38281") }
    var slot by remember { mutableStateOf("Tester") }
    val apZoneRealms = remember { mutableStateListOf<String>() }
    val shares = listOf(Triple(70, 25, 5), Triple(50, 35, 15), Triple(20, 40, 40))

    fun opts(): SoloOptionsIn {
        val modes = zoneRealms.mapNotNull { id -> m.realms.firstOrNull { it.id == id }?.mode }
        val (e, md, h) = shares[preset]
        return SoloOptionsIn(
            goal = goal, goalTarget = target.toUIntOrNull() ?: 0u, numberOfTrips = trips.toInt().toUInt(), zoneModes = modes,
            easyShare = e.toUInt(), mediumShare = md.toUInt(), hardShare = h.toUInt(), minutesPerTier = mpt.toInt().toUInt(), minDistanceM = 150u,
            questTypes = families.toList(), enabledTraps = if (trapsOn) TRAPS else emptyList(), trapRate = if (trapsOn) 30u else 0u,
            enableEffortReductions = bonus, enableScouting = bonus || fog, enableCollection = bonus, reductionPercent = 8u, fogOfWar = fog, returnHome = false,
        )
    }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Continue a game", style = MaterialTheme.typography.titleMedium)
        if (m.games.isEmpty()) Text("No saved games yet.", fontSize = 12.sp)
        m.games.forEach { g ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                Text(g.name)
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Button(onClick = { m.openGame(g.id) }) { Text("Open", fontSize = 12.sp) }
                    OutlinedButton(onClick = { m.deleteGame(g.id) }) { Text("Delete", fontSize = 12.sp) }
                }
            }
        }
        HorizontalDivider()
        Text("New game", style = MaterialTheme.typography.titleMedium)
        OutlinedTextField(name, { name = it }, label = { Text("Game name") }, singleLine = true, modifier = Modifier.fillMaxWidth())

        Text("Zones, in order (the first is where you start; later ones are unlocked by keys and tools)", fontSize = 12.sp)
        zoneRealms.forEachIndexed { i, id ->
            val r = m.realms.firstOrNull { it.id == id }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                Text("Zone ${i + 1}: ${r?.name ?: "?"} (${if (r?.mode == "drive") "car" else r?.mode})")
                TextButton(onClick = { zoneRealms.removeAt(i) }) { Text("Remove") }
            }
        }
        val free = m.realms.filter { it.scannedAtMs != null && it.id !in zoneRealms }
        if (zoneRealms.size < 6) {
            Text(if (free.isEmpty()) "Scan a realm on the Realms tab to use it here." else "Add a zone:", fontSize = 12.sp)
            free.forEach { r -> OutlinedButton(onClick = { zoneRealms.add(r.id) }) { Text("+ ${r.name} (${if (r.mode == "drive") "car" else r.mode})", fontSize = 12.sp) } }
        }

        Text("Win condition", fontSize = 13.sp)
        GOALS.chunked(2).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                row.forEach { (id, title, _) -> FilterChip(selected = goal == id, onClick = { goal = id }, label = { Text(title, fontSize = 12.sp) }) }
            }
        }
        Text(GOALS.first { it.first == goal }.third, fontSize = 12.sp, color = MaterialTheme.colorScheme.primary)
        OutlinedTextField(target, { target = it.filter(Char::isDigit) }, label = { Text("Goal target (optional)") }, singleLine = true, modifier = Modifier.fillMaxWidth())

        Text("Quests: ${trips.toInt()}", fontSize = 13.sp)
        Slider(trips, { trips = it }, valueRange = 10f..300f)
        Text("Difficulty mix", fontSize = 13.sp)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf("Relaxed", "Balanced", "Challenging").forEachIndexed { i, t -> FilterChip(selected = preset == i, onClick = { preset = i }, label = { Text(t, fontSize = 12.sp) }) }
        }
        Text("Minutes per difficulty tier: ${mpt.toInt()}", fontSize = 13.sp)
        Slider(mpt, { mpt = it }, valueRange = 5f..30f)
        Row(verticalAlignment = Alignment.CenterVertically) { Switch(fog, { fog = it }); Text("  Fog of war (discover quests)", Modifier.clickable { fog = !fog }, fontSize = 13.sp) }
        Row(verticalAlignment = Alignment.CenterVertically) { Switch(trapsOn, { trapsOn = it }); Text("  Traps (Freeze, Leash, Detour…)", Modifier.clickable { trapsOn = !trapsOn }, fontSize = 13.sp) }
        Row(verticalAlignment = Alignment.CenterVertically) { Switch(bonus, { bonus = it }); Text("  Bonus items (scouting, reductions)", Modifier.clickable { bonus = !bonus }, fontSize = 13.sp) }
        Text("Terrain", fontSize = 13.sp)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf("any" to "Any", "prefer_paved" to "Prefer paved", "paved_only" to "Paved only").forEach { (id, label) ->
                FilterChip(selected = m.surfacePref == id, onClick = { m.surfacePref = id }, label = { Text(label, fontSize = 12.sp) })
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) { Switch(m.avoidStairs, { m.avoidStairs = it }); Text("  Avoid stairs", Modifier.clickable { m.avoidStairs = !m.avoidStairs }, fontSize = 13.sp) }
        Text("Only some map data is tagged with surfaces, so \"paved\" is best effort.", fontSize = 11.sp)
        Text("Quest types", fontSize = 13.sp)
        FAMILIES.chunked(4).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                row.forEach { f -> FilterChip(selected = f in families, onClick = { if (f in families) families.remove(f) else families.add(f) }, label = { Text(f, fontSize = 11.sp) }) }
            }
        }

        val ready = zoneRealms.isNotEmpty()
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = ready, onClick = { m.startSolo(opts(), zoneRealms.toList(), name) }) { Text("Play solo") }
            OutlinedButton(enabled = ready, onClick = { m.exportYaml(opts()) }) { Text("Export YAML") }
        }
        if (!ready) Text("Add at least one zone to continue.", fontSize = 12.sp)

        HorizontalDivider()
        Text("Join an Archipelago game", style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            OutlinedTextField(url, { url = it }, label = { Text("Server") }, singleLine = true, modifier = Modifier.weight(1f))
            OutlinedTextField(slot, { slot = it }, label = { Text("Slot") }, singleLine = true, modifier = Modifier.weight(1f))
        }
        Button(onClick = { apZoneRealms.clear(); m.connectAp(url, slot) }) { Text("Connect") }
        Text("Status: ${m.apStatus}${m.apGoalSummary()?.let { "  ·  goal: $it" } ?: ""}", fontSize = 12.sp)
        if (m.apZoneModes.isNotEmpty()) {
            Text("This game needs ${m.apZoneModes.size} zone(s). Pick a matching realm for each:", fontSize = 12.sp)
            m.apZoneModes.forEachIndexed { i, mode ->
                val options = m.realms.filter { it.scannedAtMs != null && it.mode == mode }
                Text("Zone ${i + 1} (${if (mode == "drive") "car" else mode})", fontSize = 13.sp)
                if (options.isEmpty()) Text("  no scanned $mode realm: create one first", fontSize = 12.sp)
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    options.forEach { r ->
                        FilterChip(selected = apZoneRealms.getOrNull(i) == r.id, onClick = {
                            while (apZoneRealms.size <= i) apZoneRealms.add("")
                            apZoneRealms[i] = r.id
                        }, label = { Text(r.name, fontSize = 12.sp) })
                    }
                }
            }
            val complete = apZoneRealms.size == m.apZoneModes.size && apZoneRealms.none { it.isBlank() }
            Button(enabled = complete, onClick = { m.startApGame(apZoneRealms.toList(), "Archipelago: $slot") }) { Text("Start this game") }
        }
        Box(Modifier.height(24.dp))
    }
}

// -------------------------------------------------------------------- play
@Composable
fun PlayScreen(m: AppModel) {
    val hud = m.hud
    if (hud == null) {
        Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.Center) {
            Text("No game open.", style = MaterialTheme.typography.titleMedium)
            Text("Create a realm, then start a game on the New Game tab (or open a saved one).")
        }
        return
    }
    val selected = m.quests.firstOrNull { it.locationId == m.selected }
    Column(Modifier.fillMaxSize().padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("${hud.gameName}  ·  ${hud.backend}", fontSize = 12.sp)
        Text(hud.goalLabel, style = MaterialTheme.typography.titleSmall)
        LinearProgressIndicator(progress = { hud.goalProgress }, Modifier.fillMaxWidth())
        Text(
            "Quests ${hud.done}/${hud.total} · keys ${hud.keys} · ${hud.tools.joinToString().ifBlank { "no tools" }} · letters ${hud.letters.ifBlank { "-" }} · ${"%.1f".format(hud.distanceKm)} km · streak ${hud.streakDays}d",
            fontSize = 11.sp,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            m.zones.forEach { z ->
                Text(
                    "Z${z.id} ${if (z.mode == "drive") "car" else z.mode} ${if (z.unlocked) "✓" else "🔒${if (z.keysNeeded > 0u) " ${z.keysNeeded}key" else ""}${z.tool?.let { "+$it" } ?: ""}"}",
                    fontSize = 11.sp, color = if (z.unlocked) Color(0xFF2E7D32) else Color(0xFF757575),
                )
            }
        }
        (hud.traps + listOfNotNull(hud.blocked)).distinct().takeIf { it.isNotEmpty() }?.let { Text("⚠ " + it.joinToString("  ·  "), fontSize = 12.sp, color = Color(0xFFC62828)) }
        QuestMap(
            m.quests, m.realms, emptyList(), m.me,
            hud.thaw?.let { org.maplibre.android.geometry.LatLng(it.lat, it.lon) }, hud.waypoint?.let { org.maplibre.android.geometry.LatLng(it.lat, it.lon) },
            m.selected, { ll ->
                m.quests.filter { it.anchor != null && it.state != "hidden" }.minByOrNull { q ->
                    val a = q.anchor!!; val d = floatArrayOf(0f)
                    android.location.Location.distanceBetween(ll.latitude, ll.longitude, a.lat, a.lon, d); d[0]
                }?.let { m.selected = it.locationId }
            },
            Modifier.fillMaxWidth().height(260.dp),
            home = m.home?.let { LatLng(it.lat, it.lon) },
        )
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            OutlinedButton(onClick = { m.devTeleportNext() }) { Text("DEV: do next", fontSize = 11.sp) }
            OutlinedButton(onClick = { m.simPos = null }) { Text("Real GPS", fontSize = 11.sp) }
            OutlinedButton(onClick = { runCatching { m.engine.reroll(emptyList(), (kotlin.random.Random.nextLong() ushr 1).toULong()) }; m.refreshPlay() }) { Text("Reroll all", fontSize = 11.sp) }
        }
        selected?.let { q ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(8.dp)) {
                    Text("${q.name}${if (q.boss) "  (BOSS)" else ""}", style = MaterialTheme.typography.titleSmall)
                    Text("${q.place} · ${q.difficulty} · ~${q.effortMin.toInt()} min · ${q.mode}${if (q.fallback) " · fallback" else ""}", fontSize = 11.sp)
                    Text(q.detail, fontSize = 12.sp)
                    Text(q.blurb, fontSize = 11.sp)
                    q.reward?.let { Text("Reward: $it", fontSize = 12.sp, color = Color(0xFF2E7D32)) }
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        OutlinedButton(onClick = { m.devComplete(q) }) { Text("DEV: complete", fontSize = 11.sp) }
                        if (q.state != "done") OutlinedButton(onClick = { runCatching { m.engine.reroll(listOf(q.locationId), (kotlin.random.Random.nextLong() ushr 1).toULong()) }; m.refreshPlay() }) { Text("Reroll", fontSize = 11.sp) }
                    }
                }
            }
        }
        val order = listOf("progress", "open", "locked", "done", "hidden")
        LazyColumn(Modifier.weight(1f)) {
            items(m.quests.sortedBy { order.indexOf(it.state) }, key = { it.locationId }) { q ->
                Row(Modifier.fillMaxWidth().clickable { m.selected = q.locationId }.padding(vertical = 3.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Box(Modifier.size(10.dp).background(stateColor(q.state), CircleShape))
                    Column(Modifier.weight(1f)) {
                        Text(if (q.state == "hidden") "??? (undiscovered)" else q.name, fontSize = 13.sp)
                        if (q.state != "hidden") Text("${q.place} · ${q.difficulty} · ~${q.effortMin.toInt()} min${if (q.state == "progress") " · ${(q.progress * 100).toInt()}%" else ""}", fontSize = 10.sp)
                    }
                    Text(q.state, fontSize = 10.sp)
                }
            }
        }
        if (m.log.isNotEmpty()) Text(m.log.take(3).joinToString("\n"), fontSize = 11.sp, color = MaterialTheme.colorScheme.primary)
    }
}
