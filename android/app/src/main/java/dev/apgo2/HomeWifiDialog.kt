package dev.apgo2

import androidx.compose.foundation.layout.Row
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import dev.apgo2.ui.HomeOfferText

/** "You're home: add this Wi-Fi?" for a player with no home network saved (see [dev.apgo2.presence.HomeWifiOffer]). */
@Composable
internal fun HomeWifiDialog(p: PresenceController) {
    val ssid = p.homeOffer?.ssid ?: return
    AlertDialog(
        onDismissRequest = p::dismissHomeOffer,
        title = { Text(HomeOfferText.TITLE) },
        text = { Text(HomeOfferText.body(ssid)) },
        confirmButton = { TextButton(onClick = p::acceptHomeOffer) { Text(HomeOfferText.ADD) } },
        dismissButton = {
            Row {
                TextButton(onClick = p::muteHomeOffer) { Text(HomeOfferText.MUTE) }
                TextButton(onClick = p::dismissHomeOffer) { Text(HomeOfferText.LATER) }
            }
        },
    )
}
