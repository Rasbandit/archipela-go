package dev.apgo2

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import java.text.DateFormat
import java.util.Date

private const val REFRESH_MS = 3_000L

/**
 * What happened in the game and why: quests with how they were done, rewards with where they came from, traps, and (optionally)
 * the technical notes.
 */
@Composable
internal fun ActivityScreen(
    m: AppModel,
    modifier: Modifier = Modifier,
) {
    var details by remember { mutableStateOf(false) }
    // Refresh while this tab is open: new events arrive as you play.
    LaunchedEffect(m.hud?.gameName) {
        while (true) {
            m.library.refreshActivity()
            delay(REFRESH_MS)
        }
    }
    val time = remember { DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.MEDIUM) }
    val rows = m.activity.filter { ActivityFormat.shown(it.kind, details) }
    Column(modifier.fillMaxSize().padding(horizontal = 12.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Activity", style = MaterialTheme.typography.titleLarge)
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
            Text("Show GPS and app notes", fontSize = 12.sp)
            Switch(details, { details = it })
        }
        if (rows.isEmpty()) {
            Text(
                "Nothing yet. Open a game and play: quests, rewards and traps show up here with the reason.",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        LazyColumn(Modifier.weight(1f)) {
            items(rows) { e ->
                Column(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                    Text(
                        "${AwayFormat.kindLabel(e.kind)}  ·  ${time.format(Date(e.tMs))}",
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.primary,
                    )
                    if (e.detail.isNotBlank()) Text(e.detail, fontSize = 13.sp)
                }
                HorizontalDivider()
            }
        }
    }
}
