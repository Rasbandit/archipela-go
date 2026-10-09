package dev.apgo2

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.apgo2.ui.ChoiceChips
import dev.apgo2.ui.Help
import dev.apgo2.ui.LabelWithHelp
import uniffi.apgo_ffi.UnitChoice

private val UNIT_OPTIONS = listOf(UnitChoice.AUTO, UnitChoice.METRIC, UnitChoice.IMPERIAL)

private fun unitLabel(c: UnitChoice) =
    when (c) {
        UnitChoice.AUTO -> "Auto"
        UnitChoice.METRIC -> "Kilometres"
        UnitChoice.IMPERIAL -> "Miles"
    }

/** Settings that are not tied to one game. One section per concern, so more can be added below. */
@Composable
internal fun SettingsScreen(
    m: AppModel,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("Settings", style = MaterialTheme.typography.titleLarge)
        LabelWithHelp("Distance units", Help.units)
        ChoiceChips(UNIT_OPTIONS, m.units.choice, m.units::choose, ::unitLabel)
    }
}
