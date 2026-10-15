package dev.apgo2

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.presence.SetupStep
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.CampText
import dev.apgo2.ui.IconLabel
import dev.apgo2.ui.Tone

/** Your camps: add, edit (rename and move) and delete them, and reach the Wi-Fi and car setup. */
@Composable
internal fun CampsScreen(
    m: AppModel,
    onClose: () -> Unit,
) {
    var picking by rememberSaveable { mutableStateOf(false) }
    var pickId by rememberSaveable { mutableStateOf<String?>(null) }
    if (picking) {
        val close = { picking = false }
        CampPicker(
            m,
            title = if (pickId == null) CampText.ADD_TITLE else CampText.EDIT_TITLE,
            campId = pickId,
            askName = true,
            onBack = close,
            onNext = close,
        )
        return
    }
    CampList(
        m,
        onClose,
        onAdd = {
            pickId = null
            picking = true
        },
        onEdit = {
            pickId = it
            picking = true
        },
    )
}

@Composable
private fun CampList(
    m: AppModel,
    onClose: () -> Unit,
    onAdd: () -> Unit,
    onEdit: (String) -> Unit,
) {
    BackHandler { onClose() }
    var asking by remember { mutableStateOf<CampRow?>(null) }
    ConfirmDeleteCamp(asking, onConfirm = {
        asking = null
        m.campOps.delete(it.id)
    }, onDismiss = { asking = null })
    val rows = m.campOps.rows() // from memory; reads the camps, the position and the saved networks, so it follows all three
    Column(Modifier.fillMaxSize().padding(start = 8.dp, end = 8.dp, top = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        CampsHeader(rows.size, onClose, onAdd)
        Text(CampText.HELP, style = MaterialTheme.typography.bodyMedium)
        OutlinedButton(onClick = { m.setup.open(SetupStep.Wifi) }) { Text(CampText.WIFI_AND_CAR) }
        if (rows.isEmpty()) Text(CampText.UNSET, fontSize = 13.sp)
        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(rows, key = { it.id }) { r ->
                CampRowCard(r, onEdit = { onEdit(r.id) }, onWifi = { m.setup.open(SetupStep.Wifi, r.id) }, onDelete = { asking = r })
            }
        }
    }
}

@Composable
private fun CampRowCard(
    r: CampRow,
    onEdit: () -> Unit,
    onWifi: () -> Unit,
    onDelete: () -> Unit,
) {
    Card(
        Modifier.fillMaxWidth().clip(CardDefaults.shape).clickable(onClickLabel = CampText.EDIT_HINT, role = Role.Button, onClick = onEdit),
    ) {
        Row(
            Modifier.padding(start = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                ApgoIcons.Campfire,
                contentDescription = null,
                tint = ApgoPalette.camp,
                modifier = Modifier.size(26.dp),
            )
            Column(Modifier.weight(1f).padding(vertical = 10.dp)) {
                Text(r.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val detail = listOfNotNull(CampText.distance(r.distanceM), if (r.hasWifi) CampText.WIFI_SAVED else CampText.NO_WIFI)
                Text(detail.joinToString(" · "), fontSize = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            IconButton(onClick = onWifi) { Icon(ApgoIcons.Wifi, contentDescription = CampText.wifiFor(r.name)) }
            IconButton(onClick = onDelete) {
                Icon(ApgoIcons.Delete, contentDescription = CampText.deleteDescription(r.name), tint = ApgoPalette.tone(Tone.Danger))
            }
        }
    }
}

@Composable
private fun ConfirmDeleteCamp(
    camp: CampRow?,
    onConfirm: (CampRow) -> Unit,
    onDismiss: () -> Unit,
) {
    val c = camp ?: return
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(CampText.deleteTitle(c.name)) },
        text = { Text(CampText.DELETE_BODY) },
        confirmButton = { TextButton(onClick = { onConfirm(c) }) { Text(CampText.DELETE, color = ApgoPalette.tone(Tone.Danger)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(CampText.CANCEL) } },
    )
}

@Composable
private fun CampsHeader(
    count: Int,
    onClose: () -> Unit,
    onAdd: () -> Unit,
) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            IconButton(onClick = onClose) { Icon(ApgoIcons.Back, contentDescription = CampText.BACK) }
            Text(CampText.title(count), style = MaterialTheme.typography.titleMedium)
        }
        Button(onClick = onAdd) { IconLabel(CampText.ADD, ApgoIcons.Add) }
    }
}
