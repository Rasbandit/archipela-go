package dev.apgo2

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.apgo_ffi.AwayReportOut
import java.text.DateFormat
import java.util.Date

/** "While you were out": what the phone recorded since the app was last in the background. */
@Composable
internal fun AwayDialog(
    r: AwayReportOut,
    onDismiss: () -> Unit,
) {
    val time = DateFormat.getTimeInstance(DateFormat.SHORT)
    val events = r.events.filter { AwayFormat.isVisible(it.kind) }
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } },
        title = { Text("While you were out") },
        text = {
            Column {
                Text(
                    "${AwayFormat.duration(r.toMs - r.fromMs)} · ${AwayFormat.distance(r.distanceM)} · ${r.points} GPS points",
                    style = MaterialTheme.typography.bodyMedium,
                )
                if (r.simulatedPoints >
                    0u
                ) {
                    Text("${r.simulatedPoints} of them simulated", fontSize = 11.sp, color = MaterialTheme.colorScheme.error)
                }
                val counts = r.counts.filter { AwayFormat.isVisible(it.kind) }
                if (counts.isEmpty()) Text("Nothing happened.", fontSize = 12.sp)
                counts.forEach { Text("${AwayFormat.kindLabel(it.kind)}: ${it.count}", fontSize = 12.sp) }
                LazyColumn(Modifier.fillMaxWidth().heightIn(max = 220.dp)) {
                    items(events) { e ->
                        Text(
                            "${time.format(
                                Date(e.tMs),
                            )}  ${AwayFormat.kindLabel(e.kind)}${if (e.detail.isBlank()) "" else " · ${e.detail}"}",
                            fontSize = 11.sp,
                        )
                    }
                }
            }
        },
    )
}
