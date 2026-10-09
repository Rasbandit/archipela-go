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
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoChip
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.ChoiceChips
import dev.apgo2.ui.Help
import dev.apgo2.ui.HelpTip
import dev.apgo2.ui.HelpTopic
import dev.apgo2.ui.IconChoices
import dev.apgo2.ui.IconLabel
import dev.apgo2.ui.LabelWithHelp
import dev.apgo2.ui.PLAY_MODES
import dev.apgo2.ui.modeLabel
import uniffi.apgo_ffi.GoalPickIn
import uniffi.apgo_ffi.RealmOut
import uniffi.apgo_ffi.SoloOptionsIn

private const val ANY = "any"
private const val ALL = "all"
private const val AT_LEAST = "at_least"
private const val MAX_ZONES = 6
private const val MIN_DISTANCE_M = 150u
private const val TRAP_RATE_PERCENT = 30u
private const val REDUCTION_PERCENT = 8u
private const val DEFAULT_TRIPS = 60f
private const val DEFAULT_MINUTES_PER_TIER = 10f
private const val MAX_TARGET_DIGITS = 4
private const val DEFAULT_NEED = 2
private const val BALANCED_PRESET = 1
private val TRIPS_RANGE = 10f..300f
private val MINUTES_RANGE = 5f..30f

private val FAMILIES = listOf("reach", "dwell", "landmark", "trail", "park", "water", "courier", "explore", "steps", "away")

// Quest types that need no found places: they are made from streets and open map.
private val NEEDS_NO_FINDS = setOf("reach", "courier", "explore", "steps", "away")

// The win conditions, in the game's own order: id, title.
private val GOALS =
    listOf(
        "macguffin_short" to "Letter Hunt",
        "macguffin_long" to "Letter Hunt XL",
        "all_trips" to "Completionist",
        "boss" to "The Big One",
        "treasure_hunt" to "Treasure Hunt",
        "zone_conqueror" to "Zone Conqueror",
        "well_rounded" to "Well Rounded",
        "quest_dex" to "Quest-dex",
        "marathon" to "Marathon",
        "explorer" to "Explorer",
        "streak" to "Daily Habit",
        "boss_rush" to "Boss Rush",
    )

/** A goal that counts something: what it counts and the usual number. */
private data class GoalNumber(
    val unit: String,
    val default: Int,
)

private val GOAL_NUMBERS =
    mapOf(
        "zone_conqueror" to GoalNumber("percent of every zone", 60),
        "quest_dex" to GoalNumber("kinds of quest", 15),
        "marathon" to GoalNumber("kilometers", 42),
        "explorer" to GoalNumber("map cells", 300),
        "streak" to GoalNumber("days in a row", 7),
        "boss_rush" to GoalNumber("hard quests", 5),
    )

private val TRAPS = listOf("freeze", "fog", "shuffle", "silence", "leash", "detour", "toll", "slow", "honor")

/** How the quests split over the three difficulty tiers, in percent. */
private data class DifficultyMix(
    val label: String,
    val easy: Int,
    val medium: Int,
    val hard: Int,
)

private val MIXES =
    listOf(DifficultyMix("Relaxed", 70, 25, 5), DifficultyMix("Balanced", 50, 35, 15), DifficultyMix("Challenging", 20, 40, 40))
private val TERRAIN_LABELS = mapOf(ANY to "Any", "prefer_paved" to "Prefer paved", "paved_only" to "Paved only")
private val REQUIREMENT_LABELS = mapOf(ANY to "Any one", ALL to "All of them", AT_LEAST to "At least…")

/** A zone being set up: a realm, how you travel there, and the quest types it uses. */
private data class ZoneDraft(
    val realmId: String,
    val mode: String,
    val types: Set<String>,
)

// Everything the New Game screen is asking for. It lives as long as the screen is on view.
@Stable
private class NewGameForm {
    var name by mutableStateOf("My game")
    val zones = mutableStateListOf<ZoneDraft>()
    val goals = mutableStateListOf("macguffin_short")
    val targets = mutableStateMapOf<String, String>()
    var requirement by mutableStateOf(ANY)
    var need by mutableIntStateOf(DEFAULT_NEED)
    var trips by mutableFloatStateOf(DEFAULT_TRIPS)
    var preset by mutableIntStateOf(BALANCED_PRESET)
    var minutesPerTier by mutableFloatStateOf(DEFAULT_MINUTES_PER_TIER)
    var fog by mutableStateOf(false)
    var trapsOn by mutableStateOf(true)
    var bonus by mutableStateOf(true)
    var url by mutableStateOf("localhost:38281")
    var slot by mutableStateOf("Tester")
    val apZoneRealms = mutableStateListOf<String>()

    // There is always at least one goal.
    fun toggleGoal(id: String) {
        if (id in goals) {
            if (goals.size > 1) goals.remove(id)
        } else {
            goals.add(id)
            goals.sortBy { g -> GOALS.indexOfFirst { it.first == g } }
        }
    }

    fun pickApRealm(
        zoneIndex: Int,
        realmId: String,
    ) {
        while (apZoneRealms.size <= zoneIndex) apZoneRealms.add("")
        apZoneRealms[zoneIndex] = realmId
    }

    fun options(): SoloOptionsIn {
        val mix = MIXES[preset]
        return SoloOptionsIn(
            goals = goals.map { GoalPickIn(it, targets[it]?.toUIntOrNull() ?: 0u) },
            goalRequirement = if (goals.size == 1) ANY else requirement,
            goalNeed = need.coerceIn(1, goals.size).toUInt(),
            numberOfTrips = trips.toInt().toUInt(),
            zoneModes = zones.map { it.mode },
            easyShare = mix.easy.toUInt(),
            mediumShare = mix.medium.toUInt(),
            hardShare = mix.hard.toUInt(),
            minutesPerTier = minutesPerTier.toInt().toUInt(),
            minDistanceM = MIN_DISTANCE_M,
            questTypes = FAMILIES,
            zoneQuestTypes = zones.map { z -> FAMILIES.filter { it in z.types } },
            enabledTraps = if (trapsOn) TRAPS else emptyList(),
            trapRate = if (trapsOn) TRAP_RATE_PERCENT else 0u,
            enableEffortReductions = bonus,
            enableScouting = bonus || fog,
            enableCollection = bonus,
            reductionPercent = REDUCTION_PERCENT,
            fogOfWar = fog,
            returnHome = false,
        )
    }
}

// Quest types a realm can serve, with how many places were found for each.
private fun findsByFamily(
    m: AppModel,
    realmId: String,
): Map<String, Int> =
    m.offers[realmId]
        .orEmpty()
        .groupBy { it.family }
        .mapValues { (_, kinds) -> kinds.sumOf { it.count.toInt() } }

private fun availableTypes(
    m: AppModel,
    realmId: String,
): Set<String> {
    val found = findsByFamily(m, realmId)
    return FAMILIES.filter { it in NEEDS_NO_FINDS || (found[it] ?: 0) > 0 }.toSet()
}

/** The New Game tab: zones, win conditions and options, then Play solo, Export YAML or join an Archipelago game. */
@Composable
internal fun NewGameScreen(
    m: AppModel,
    modifier: Modifier = Modifier,
) {
    val form = remember { NewGameForm() }
    Column(
        modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 10.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text("New game", style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(
            form.name,
            { form.name = it },
            label = { Text("Game name") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        ZonesSection(
            m,
            form.zones,
            onChange = { i, z -> form.zones[i] = z },
            onAdd = { form.zones.add(it) },
            onRemove = { form.zones.removeAt(it) },
        )
        GoalsSection(
            form.goals,
            form.targets,
            onToggle = form::toggleGoal,
            onTarget = { id, v -> form.targets[id] = v },
            requirement = form.requirement,
            onRequirement = { form.requirement = it },
            need = form.need,
            onNeed = { form.need = it },
        )
        PlaySettings(m, form)
        StartButtons(m, form)
        HorizontalDivider()
        ArchipelagoSection(m, form)
        Box(Modifier.height(24.dp))
    }
}

@Composable
private fun PlaySettings(
    m: AppModel,
    form: NewGameForm,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LabelWithHelp("Number of quests: ${form.trips.toInt()}", Help.questCount)
        Slider(form.trips, { form.trips = it }, valueRange = TRIPS_RANGE)
        LabelWithHelp("Difficulty mix", Help.difficulty)
        ChoiceChips(MIXES.indices.toList(), form.preset, { form.preset = it }, { MIXES[it].label })
        LabelWithHelp("Minutes per difficulty tier: ${form.minutesPerTier.toInt()}", Help.minutesPerTier)
        Slider(form.minutesPerTier, { form.minutesPerTier = it }, valueRange = MINUTES_RANGE)
        SwitchRow("Fog of war", Help.fog, form.fog) { form.fog = it }
        SwitchRow("Traps", Help.traps, form.trapsOn) { form.trapsOn = it }
        SwitchRow("Bonus items", Help.bonus, form.bonus) { form.bonus = it }
        LabelWithHelp("Terrain", Help.terrain)
        ChoiceChips(TERRAIN_LABELS.keys.toList(), m.surfacePref, { m.surfacePref = it }, { TERRAIN_LABELS[it] ?: it })
        SwitchRow("Avoid stairs", Help.stairs, m.avoidStairs) { m.avoidStairs = it }
    }
}

@Composable
private fun StartButtons(
    m: AppModel,
    form: NewGameForm,
) {
    val ready = form.zones.isNotEmpty()
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = ready, onClick = {
                m.library.startSolo(form.options(), form.zones.map { it.realmId }, form.name)
            }) { Text("Play solo") }
            OutlinedButton(enabled = ready, onClick = { m.library.exportYaml(form.options()) }) { Text("Export YAML") }
        }
        if (!ready) Text("Add at least one zone to continue.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun SwitchRow(
    label: String,
    help: HelpTopic,
    on: Boolean,
    onChange: (Boolean) -> Unit,
) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
        LabelWithHelp(label, help)
        Switch(on, onChange)
    }
}

// ------------------------------------------------------------------------------------------------------------------ zones

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ZonesSection(
    m: AppModel,
    zones: List<ZoneDraft>,
    onChange: (Int, ZoneDraft) -> Unit,
    onAdd: (ZoneDraft) -> Unit,
    onRemove: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LabelWithHelp("Zones", Help.zones, style = MaterialTheme.typography.titleMedium)
        Text(
            "Zone 1 is where you start. Later zones open as you find keys.",
            fontSize = 12.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        zones.forEachIndexed { i, z ->
            val realm = m.realmOps.shown.firstOrNull { it.id == z.realmId }
            ZoneCard(m, i, z, realm, onChange = { onChange(i, it) }, onRemove = { onRemove(i) })
        }
        val scanned = m.realmOps.shown.filter { it.scannedAtMs != null }
        if (zones.size < MAX_ZONES) {
            Text(
                if (scanned.isEmpty()) "Scan a realm on the Realms tab to use it here." else "Add a zone from one of your realms:",
                fontSize = 12.sp,
            )
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                scanned.forEach { r ->
                    OutlinedButton(
                        onClick = { onAdd(ZoneDraft(r.id, "walk", availableTypes(m, r.id))) },
                    ) { IconLabel(r.name, ApgoIcons.realm(r.icon)) }
                }
            }
        }
    }
}

@Composable
private fun ZoneCard(
    m: AppModel,
    index: Int,
    zone: ZoneDraft,
    realm: RealmOut?,
    onChange: (ZoneDraft) -> Unit,
    onRemove: () -> Unit,
) {
    val available = availableTypes(m, zone.realmId)
    Card(
        Modifier.fillMaxWidth(),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            ZoneHeader(index, realm, onRemove)
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
                modifier = Modifier.fillMaxWidth(),
            ) {
                LabelWithHelp("Travel by ${modeLabel(zone.mode)}", Help.travelMode)
                IconChoices(PLAY_MODES, zone.mode, { onChange(zone.copy(mode = it)) }, ApgoIcons::mode, ::modeLabel)
            }
            LabelWithHelp("Quest types", Help.questTypes)
            QuestTypeChips(findsByFamily(m, zone.realmId), available, zone, onChange)
            if (zone.types.none { it in available }) {
                Text(
                    "Pick at least one type, or walking-to-a-point quests are used.",
                    fontSize = 11.sp,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}

@Composable
private fun ZoneHeader(
    index: Int,
    realm: RealmOut?,
    onRemove: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Icon(
            ApgoIcons.realm(realm?.icon),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.size(24.dp),
        )
        Column(Modifier.weight(1f)) {
            Text("Zone ${index + 1}: ${realm?.name ?: "realm removed"}", style = MaterialTheme.typography.titleSmall)
            Text(
                if (index == 0) "You start here" else "Opens with keys",
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        IconButton(
            onClick = onRemove,
        ) { Icon(ApgoIcons.Remove, contentDescription = "Remove zone ${index + 1}", tint = ApgoPalette.danger) }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun QuestTypeChips(
    found: Map<String, Int>,
    available: Set<String>,
    zone: ZoneDraft,
    onChange: (ZoneDraft) -> Unit,
) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        FAMILIES.forEach { f ->
            val here = f in available
            val label = if (f in NEEDS_NO_FINDS) f else "$f ${found[f] ?: 0}"
            ApgoChip(
                label,
                f in zone.types && here,
                { onChange(zone.copy(types = if (f in zone.types) zone.types - f else zone.types + f)) },
                textSize = 11.sp,
                enabled = here || f in zone.types,
                icon = ApgoIcons.familyIcons[f],
            )
        }
    }
}

// ------------------------------------------------------------------------------------------------------------------ goals

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GoalsSection(
    goals: List<String>,
    targets: Map<String, String>,
    onToggle: (String) -> Unit,
    onTarget: (String, String) -> Unit,
    requirement: String,
    onRequirement: (String) -> Unit,
    need: Int,
    onNeed: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LabelWithHelp("Win conditions", Help.goals, style = MaterialTheme.typography.titleMedium)
        Text("Choose one or more.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            GOALS.forEach { (id, title) ->
                ApgoChip(title, id in goals, { onToggle(id) }, textSize = 12.sp, icon = if (id in goals) ApgoIcons.Check else null)
            }
        }
        // What each chosen goal means, and its number if it counts something.
        goals.forEach { id -> GoalDetail(id, targets[id] ?: "") { onTarget(id, it) } }
        if (goals.size >= 2) GoalRule(goals.size, requirement, onRequirement, need, onNeed)
    }
}

@Composable
private fun GoalDetail(
    id: String,
    target: String,
    onTarget: (String) -> Unit,
) {
    val title = GOALS.first { it.first == id }.second
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
            Help.goalDescriptions[id]?.let { HelpTip(it) }
        }
        GOAL_NUMBERS[id]?.let { n ->
            OutlinedTextField(
                target,
                { onTarget(it.filter(Char::isDigit).take(MAX_TARGET_DIGITS)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                label = { Text("${n.unit} (usually ${n.default})") },
                trailingIcon = { HelpTip(Help.goalTarget) },
            )
        }
    }
}

// How several goals combine: any one, all of them, or at least N. Shown only when two or more goals are chosen.
@Composable
private fun GoalRule(
    goalCount: Int,
    requirement: String,
    onRequirement: (String) -> Unit,
    need: Int,
    onNeed: (Int) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LabelWithHelp("You win when you finish", Help.goalRule)
        ChoiceChips(REQUIREMENT_LABELS.keys.toList(), requirement, onRequirement, { REQUIREMENT_LABELS[it] ?: it })
        if (requirement == AT_LEAST) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { onNeed((need - 1).coerceAtLeast(1)) }) { Text("−") }
                Text("${need.coerceIn(1, goalCount)} of $goalCount goals", style = MaterialTheme.typography.titleSmall)
                OutlinedButton(onClick = { onNeed((need + 1).coerceAtMost(goalCount)) }) { Text("+") }
            }
        }
        Text(
            when (requirement) {
                ANY -> "The first of these $goalCount goals you finish wins."
                ALL -> "You must finish all $goalCount goals."
                else -> "Finish ${need.coerceIn(1, goalCount)} of the $goalCount goals to win."
            },
            fontSize = 12.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

// ------------------------------------------------------------------------------------------------------------------ Archipelago

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ArchipelagoSection(
    m: AppModel,
    form: NewGameForm,
    modifier: Modifier = Modifier,
) {
    val connect = rememberLanAwareConnect({ m.ap.connect(it, form.slot) }, { m.ap.hint = it })
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LabelWithHelp("Join an Archipelago game", Help.archipelago, style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            OutlinedTextField(form.url, { form.url = it }, label = { Text("Server") }, singleLine = true, modifier = Modifier.weight(1f))
            OutlinedTextField(form.slot, { form.slot = it }, label = { Text("Slot") }, singleLine = true, modifier = Modifier.weight(1f))
        }
        Button(onClick = {
            form.apZoneRealms.clear()
            connect(form.url)
        }) { Text("Connect") }
        Text("Status: ${m.ap.status}${m.ap.goalSummary()?.let { "  ·  goal: $it" } ?: ""}", fontSize = 12.sp)
        m.ap.hint?.let { Text(it, fontSize = 12.sp, color = MaterialTheme.colorScheme.error) }
        if (m.ap.zoneModes.isNotEmpty()) {
            Text("This game needs ${m.ap.zoneModes.size} zone(s). Pick a realm for each:", fontSize = 12.sp)
            m.ap.zoneModes.forEachIndexed { i, mode ->
                val options = m.realmOps.shown.filter { it.scannedAtMs != null }
                Text("Zone ${i + 1} (${modeLabel(mode)})", style = MaterialTheme.typography.titleSmall)
                if (options.isEmpty()) Text("No scanned realm yet: create one first.", fontSize = 12.sp)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    options.forEach { r ->
                        ApgoChip(r.name, form.apZoneRealms.getOrNull(i) == r.id, { form.pickApRealm(i, r.id) })
                    }
                }
            }
            val complete = form.apZoneRealms.size == m.ap.zoneModes.size && form.apZoneRealms.none { it.isBlank() }
            Button(enabled = complete, onClick = {
                m.ap.startGame(form.apZoneRealms.toList(), "Archipelago: ${form.slot}")
            }) { Text("Start this game") }
        }
    }
}
