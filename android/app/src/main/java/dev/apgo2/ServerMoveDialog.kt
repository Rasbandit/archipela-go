package dev.apgo2

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.FeedbackText
import dev.apgo2.ui.Tone

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

/**
 * Shown while the open game's home-network server waits for local network access (#171): the reason and a button that asks for it,
 * or opens the app's settings once Android no longer shows its dialog. A grant connects at once; one made in Settings connects
 * when the app comes back (ApController.onForeground).
 */
@Composable
internal fun LanPrompt(m: AppModel) {
    val ctx = LocalContext.current
    val ask = rememberPermissionAsk(LOCAL_NETWORK_PERMISSION, { !ctx.lacksLocalNetwork() }) { if (it) m.ap.connectHeld() }
    if (m.ap.lanHeld == null) return
    val label = if (ask.action == PermissionAsk.OpenSettings) "Open settings" else "Allow"
    // Already granted (in Settings, or from New Game's prompt) while held: connect rather than a button that does nothing.
    val allow = if (ask.action == PermissionAsk.Granted) m.ap::connectHeld else ask::ask
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FeedbackText(LAN_HELD, Tone.Warning, Modifier.weight(1f))
        TextButton(onClick = allow) { Text(label, fontSize = 12.sp) }
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
