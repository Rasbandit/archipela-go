package dev.apgo2

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoChip
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.ChoiceChips
import dev.apgo2.ui.Help
import dev.apgo2.ui.HelpTip
import dev.apgo2.ui.IconChoices
import dev.apgo2.ui.IconLabel
import dev.apgo2.ui.LabelWithHelp
import dev.apgo2.ui.PLAY_MODES
import dev.apgo2.ui.modeLabel
import uniffi.apgo_ffi.GoalPickIn
import uniffi.apgo_ffi.RealmOut
import uniffi.apgo_ffi.SoloOptionsIn

private val FAMILIES = listOf("reach", "dwell", "landmark", "trail", "park", "water", "courier", "explore", "steps", "away")

/** Quest types that need no found places: they are made from streets and open map. */
private val NEEDS_NO_FINDS = setOf("reach", "courier", "explore", "steps", "away")

/** The win conditions, in the game's own order: id, title. */
private val GOALS = listOf(
    "macguffin_short" to "Letter Hunt", "macguffin_long" to "Letter Hunt XL", "all_trips" to "Completionist", "boss" to "The Big One",
    "treasure_hunt" to "Treasure Hunt", "zone_conqueror" to "Zone Conqueror", "well_rounded" to "Well Rounded", "quest_dex" to "Quest-dex",
    "marathon" to "Marathon", "explorer" to "Explorer", "streak" to "Daily Habit", "boss_rush" to "Boss Rush",
)

/** A goal that counts something: what it counts and the usual number. */
private data class GoalNumber(val unit: String, val default: Int)

private val GOAL_NUMBERS = mapOf(
    "zone_conqueror" to GoalNumber("percent of every zone", 60), "quest_dex" to GoalNumber("kinds of quest", 15), "marathon" to GoalNumber("kilometers", 42),
    "explorer" to GoalNumber("map cells", 300), "streak" to GoalNumber("days in a row", 7), "boss_rush" to GoalNumber("hard quests", 5),
)

private val TRAPS = listOf("freeze", "fog", "shuffle", "silence", "leash", "detour", "toll", "slow", "honor")

/** A zone being set up: a realm, how you travel there, and the quest types it uses. */
private data class ZoneDraft(val realmId: String, val mode: String, val types: Set<String>)

/** Quest types a realm can serve, with how many places were found for each. */
private fun findsByFamily(m: AppModel, realmId: String): Map<String, Int> =
    m.offers[realmId].orEmpty().groupBy { it.family }.mapValues { (_, kinds) -> kinds.sumOf { it.count.toInt() } }

private fun availableTypes(m: AppModel, realmId: String): Set<String> {
    val found = findsByFamily(m, realmId)
    return FAMILIES.filter { it in NEEDS_NO_FINDS || (found[it] ?: 0) > 0 }.toSet()
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun NewGameScreen(m: AppModel) {
    var name by remember { mutableStateOf("My game") }
    val zones = remember { mutableStateListOf<ZoneDraft>() }
    val goals = remember { mutableStateListOf("macguffin_short") }
    val targets = remember { mutableStateMapOf<String, String>() }
    var requirement by remember { mutableStateOf("any") }
    var need by remember { mutableIntStateOf(2) }
    var trips by remember { mutableFloatStateOf(60f) }
    var preset by remember { mutableIntStateOf(1) }
    var mpt by remember { mutableFloatStateOf(10f) }
    var fog by remember { mutableStateOf(false) }
    var trapsOn by remember { mutableStateOf(true) }
    var bonus by remember { mutableStateOf(true) }
    var awayZoneOnly by remember { mutableStateOf(true) }
    var awayAuto by remember { mutableStateOf(true) }
    var awayMeters by remember { mutableStateOf("1000") }
    var url by remember { mutableStateOf("localhost:38281") }
    var slot by remember { mutableStateOf("Tester") }
    val apZoneRealms = remember { mutableStateListOf<String>() }
    val shares = listOf(Triple(70, 25, 5), Triple(50, 35, 15), Triple(20, 40, 40))

    fun opts(): SoloOptionsIn {
        val (e, md, h) = shares[preset]
        return SoloOptionsIn(
            goals = goals.map { GoalPickIn(it, targets[it]?.toUIntOrNull() ?: 0u) },
            goalRequirement = if (goals.size == 1) "any" else requirement,
            goalNeed = need.coerceIn(1, goals.size).toUInt(),
            numberOfTrips = trips.toInt().toUInt(), zoneModes = zones.map { it.mode },
            easyShare = e.toUInt(), mediumShare = md.toUInt(), hardShare = h.toUInt(), minutesPerTier = mpt.toInt().toUInt(), minDistanceM = 150u,
            questTypes = FAMILIES, zoneQuestTypes = zones.map { z -> FAMILIES.filter { it in z.types } },
            enabledTraps = if (trapsOn) TRAPS else emptyList(), trapRate = if (trapsOn) 30u else 0u,
            enableEffortReductions = bonus, enableScouting = bonus || fog, enableCollection = bonus, reductionPercent = 8u, fogOfWar = fog, returnHome = false,
        )
    }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 10.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("New game", style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(name, { name = it }, label = { Text("Game name") }, singleLine = true, modifier = Modifier.fillMaxWidth())

        ZonesSection(m, zones)
        GoalsSection(goals, targets, requirement, { requirement = it }, need, { need = it })

        // ---- how the game plays
        LabelWithHelp("Number of quests: ${trips.toInt()}", Help.questCount)
        Slider(trips, { trips = it }, valueRange = 10f..300f)
        LabelWithHelp("Difficulty mix", Help.difficulty)
        ChoiceChips(listOf(0, 1, 2), preset, { preset = it }, { listOf("Relaxed", "Balanced", "Challenging")[it] })
        LabelWithHelp("Minutes per difficulty tier: ${mpt.toInt()}", Help.minutesPerTier)
        Slider(mpt, { mpt = it }, valueRange = 5f..30f)
        SwitchRow("Fog of war", Help.fog, fog) { fog = it }
        SwitchRow("Traps", Help.traps, trapsOn) { trapsOn = it }
        SwitchRow("Bonus items", Help.bonus, bonus) { bonus = it }
        LabelWithHelp("Terrain", Help.terrain)
        ChoiceChips(listOf("any", "prefer_paved", "paved_only"), m.surfacePref, { m.surfacePref = it }, { mapOf("any" to "Any", "prefer_paved" to "Prefer paved", "paved_only" to "Paved only")[it] ?: it })
        SwitchRow("Avoid stairs", Help.stairs, m.avoidStairs) { m.avoidStairs = it }

        Text("Time away", style = MaterialTheme.typography.titleMedium)
        SwitchRow("Only count time inside a zone", Help.awayZone, awayZoneOnly) { awayZoneOnly = it }
        SwitchRow("Pick the distance automatically", Help.awayDistance, awayAuto) { awayAuto = it }
        if (!awayAuto) OutlinedTextField(awayMeters, { awayMeters = it.filter(Char::isDigit).take(5) }, label = { Text("Away distance (metres)") }, singleLine = true, keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Number), modifier = Modifier.fillMaxWidth())

        val ready = zones.isNotEmpty()
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = ready, onClick = { m.startSolo(opts(), zones.map { it.realmId }, name, awayZoneOnly, AwaySettings.distance(awayAuto, awayMeters)) }) { Text("Play solo") }
            OutlinedButton(enabled = ready, onClick = { m.exportYaml(opts()) }) { Text("Export YAML") }
        }
        if (!ready) Text("Add at least one zone to continue.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)

        HorizontalDivider()
        ArchipelagoSection(m, url, { url = it }, slot, { slot = it }, apZoneRealms, awayZoneOnly, AwaySettings.distance(awayAuto, awayMeters))
        Box(Modifier.height(24.dp))
    }
}

@Composable
private fun SwitchRow(label: String, help: dev.apgo2.ui.HelpTopic, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
        LabelWithHelp(label, help)
        Switch(on, onChange)
    }
}

// ------------------------------------------------------------------------------------------------------------------ zones

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ZonesSection(m: AppModel, zones: MutableList<ZoneDraft>) {
    LabelWithHelp("Zones", Help.zones, style = MaterialTheme.typography.titleMedium)
    Text("Zone 1 is where you start. Later zones open as you find keys.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    zones.forEachIndexed { i, z ->
        val realm = m.shownRealms.firstOrNull { it.id == z.realmId }
        ZoneCard(m, i, z, realm, onChange = { zones[i] = it }, onRemove = { zones.removeAt(i) })
    }
    val scanned = m.shownRealms.filter { it.scannedAtMs != null }
    if (zones.size < 6) {
        Text(if (scanned.isEmpty()) "Scan a realm on the Realms tab to use it here." else "Add a zone from one of your realms:", fontSize = 12.sp)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            scanned.forEach { r ->
                OutlinedButton(onClick = { zones.add(ZoneDraft(r.id, "walk", availableTypes(m, r.id))) }) { IconLabel(r.name, ApgoIcons.realm(r.icon)) }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ZoneCard(m: AppModel, index: Int, zone: ZoneDraft, realm: RealmOut?, onChange: (ZoneDraft) -> Unit, onRemove: () -> Unit) {
    val found = findsByFamily(m, zone.realmId)
    val available = availableTypes(m, zone.realmId)
    Card(Modifier.fillMaxWidth(), border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(ApgoIcons.realm(realm?.icon), contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(24.dp))
                Column(Modifier.weight(1f)) {
                    Text("Zone ${index + 1}: ${realm?.name ?: "realm removed"}", style = MaterialTheme.typography.titleSmall)
                    Text(if (index == 0) "You start here" else "Opens with keys", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                IconButton(onClick = onRemove) { Icon(ApgoIcons.Remove, contentDescription = "Remove zone ${index + 1}", tint = ApgoPalette.danger) }
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                LabelWithHelp("Travel by ${modeLabel(zone.mode)}", Help.travelMode)
                IconChoices(PLAY_MODES, zone.mode, { onChange(zone.copy(mode = it)) }, ApgoIcons::mode, ::modeLabel)
            }
            LabelWithHelp("Quest types", Help.questTypes)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                FAMILIES.forEach { f ->
                    val here = f in available
                    val label = if (f in NEEDS_NO_FINDS) f else "$f ${found[f] ?: 0}"
                    ApgoChip(
                        label, f in zone.types && here, { onChange(zone.copy(types = if (f in zone.types) zone.types - f else zone.types + f)) },
                        textSize = 11.sp, enabled = here || f in zone.types, icon = ApgoIcons.familyIcons[f],
                    )
                }
            }
            if (zone.types.none { it in available }) Text("Pick at least one type, or walking-to-a-point quests are used.", fontSize = 11.sp, color = MaterialTheme.colorScheme.error)
        }
    }
}

// ------------------------------------------------------------------------------------------------------------------ goals

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GoalsSection(
    goals: MutableList<String>, targets: MutableMap<String, String>, requirement: String, onRequirement: (String) -> Unit, need: Int, onNeed: (Int) -> Unit,
) {
    LabelWithHelp("Win conditions", Help.goals, style = MaterialTheme.typography.titleMedium)
    Text("Choose one or more.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        GOALS.forEach { (id, title) ->
            ApgoChip(
                title, id in goals,
                { if (id in goals) { if (goals.size > 1) goals.remove(id) } else goals.add(id) }, // there is always at least one goal
                textSize = 12.sp, icon = if (id in goals) ApgoIcons.Check else null,
            )
        }
    }
    goals.sortBy { id -> GOALS.indexOfFirst { it.first == id } }
    // What each chosen goal means, and its number if it counts something.
    goals.forEach { id ->
        val title = GOALS.first { it.first == id }.second
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
                Help.goalDescriptions[id]?.let { HelpTip(it) }
            }
            GOAL_NUMBERS[id]?.let { n ->
                OutlinedTextField(
                    targets[id] ?: "", { targets[id] = it.filter(Char::isDigit).take(4) }, singleLine = true, modifier = Modifier.fillMaxWidth(),
                    label = { Text("${n.unit} (usually ${n.default})") },
                    trailingIcon = { HelpTip(Help.goalTarget) },
                )
            }
        }
    }
    if (goals.size >= 2) {
        LabelWithHelp("You win when you finish", Help.goalRule)
        ChoiceChips(listOf("any", "all", "at_least"), requirement, onRequirement, { mapOf("any" to "Any one", "all" to "All of them", "at_least" to "At least…")[it] ?: it })
        if (requirement == "at_least") {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { onNeed((need - 1).coerceAtLeast(1)) }) { Text("−") }
                Text("${need.coerceIn(1, goals.size)} of ${goals.size} goals", style = MaterialTheme.typography.titleSmall)
                OutlinedButton(onClick = { onNeed((need + 1).coerceAtMost(goals.size)) }) { Text("+") }
            }
        }
        Text(
            when (requirement) {
                "any" -> "The first of these ${goals.size} goals you finish wins."
                "all" -> "You must finish all ${goals.size} goals."
                else -> "Finish ${need.coerceIn(1, goals.size)} of the ${goals.size} goals to win."
            },
            fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

// ------------------------------------------------------------------------------------------------------------------ Archipelago

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ArchipelagoSection(m: AppModel, url: String, onUrl: (String) -> Unit, slot: String, onSlot: (String) -> Unit, apZoneRealms: MutableList<String>, awayZoneOnly: Boolean, awayDistanceM: UInt) {
    LabelWithHelp("Join an Archipelago game", Help.archipelago, style = MaterialTheme.typography.titleMedium)
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        OutlinedTextField(url, onUrl, label = { Text("Server") }, singleLine = true, modifier = Modifier.weight(1f))
        OutlinedTextField(slot, onSlot, label = { Text("Slot") }, singleLine = true, modifier = Modifier.weight(1f))
    }
    Button(onClick = { apZoneRealms.clear(); m.connectAp(url, slot) }) { Text("Connect") }
    Text("Status: ${m.apStatus}${m.apGoalSummary()?.let { "  ·  goal: $it" } ?: ""}", fontSize = 12.sp)
    if (m.apZoneModes.isNotEmpty()) {
        Text("This game needs ${m.apZoneModes.size} zone(s). Pick a realm for each:", fontSize = 12.sp)
        m.apZoneModes.forEachIndexed { i, mode ->
            val options = m.shownRealms.filter { it.scannedAtMs != null }
            Text("Zone ${i + 1} (${modeLabel(mode)})", style = MaterialTheme.typography.titleSmall)
            if (options.isEmpty()) Text("No scanned realm yet: create one first.", fontSize = 12.sp)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                options.forEach { r ->
                    ApgoChip(r.name, apZoneRealms.getOrNull(i) == r.id, {
                        while (apZoneRealms.size <= i) apZoneRealms.add("")
                        apZoneRealms[i] = r.id
                    })
                }
            }
        }
        val complete = apZoneRealms.size == m.apZoneModes.size && apZoneRealms.none { it.isBlank() }
        Button(enabled = complete, onClick = { m.startApGame(apZoneRealms.toList(), "Archipelago: $slot", awayZoneOnly, awayDistanceM) }) { Text("Start this game") }
    }
}
