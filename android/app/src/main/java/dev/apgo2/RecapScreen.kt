package dev.apgo2

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.apgo2.ui.Help
import uniffi.apgo_ffi.RecapOut

/** "Welcome home": how the last walk went, shown over the app until dismissed. */
@Composable
internal fun RecapScreen(
    r: RecapOut,
    onDone: () -> Unit,
) {
    BackHandler(onBack = onDone)
    Surface(Modifier.fillMaxSize()) {
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Text(Help.RECAP_TITLE, style = MaterialTheme.typography.headlineMedium) }
            item {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    RecapFormat.stats(r).forEach { (label, value) -> StatTile(label, value) }
                }
            }
            items(RecapFormat.sections(r)) { (label, lines) ->
                Text(label, style = MaterialTheme.typography.titleSmall)
                lines.forEach { Text(it) }
            }
            item { Button(onClick = onDone, modifier = Modifier.fillMaxWidth()) { Text("Done") } }
        }
    }
}

@Composable
private fun StatTile(
    label: String,
    value: String,
) {
    Card {
        Text(value, Modifier.padding(start = 12.dp, top = 8.dp, end = 12.dp), style = MaterialTheme.typography.titleLarge)
        Text(label, Modifier.padding(start = 12.dp, end = 12.dp, bottom = 8.dp), style = MaterialTheme.typography.labelMedium)
    }
}
