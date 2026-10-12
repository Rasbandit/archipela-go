package dev.apgo2

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoIcons

/**
 * The open Archipelago game's server address, editable: an archipelago.gg room can lose its port and move to another one. The game
 * keeps its seed and slot, so it carries on there. Nothing is shown for a game without a saved server.
 */
@Composable
internal fun ServerButton(m: AppModel) {
    // Remembered whether or not the dialog shows: the connect it returns outlives the dialog (a permission prompt, a DNS lookup).
    val move = rememberLanAwareConnect({ m.ap.moveServer(it) }, { m.ap.hint = it })
    val current = m.ap.savedServer?.address ?: return
    var editing by rememberSaveable { mutableStateOf(false) }
    TextButton(onClick = { editing = true }) {
        Icon(ApgoIcons.Server, contentDescription = null, modifier = Modifier.size(16.dp))
        Text(" Server", fontSize = 12.sp)
    }
    if (editing) {
        ServerMoveDialog(current, onMove = {
            editing = false
            move(it)
        }, onDismiss = { editing = false })
    }
}

@Composable
private fun ServerMoveDialog(
    current: String,
    onMove: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var address by rememberSaveable { mutableStateOf(current) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Archipelago server") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("If the room moved to a new port, change it here. Your game and progress stay.", fontSize = 12.sp)
                OutlinedTextField(address, { address = it }, label = { Text("Server") }, singleLine = true)
            }
        },
        confirmButton = {
            TextButton(onClick = { onMove(address) }, enabled = address.isNotBlank() && address.trim() != current) { Text("Reconnect") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
