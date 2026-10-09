package dev.apgo2

import android.content.ClipData
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.ui.ApgoIcons
import kotlinx.coroutines.launch

// A tab of the bottom bar; its position in the list is its AppTab index.
private data class NavTab(
    val label: String,
    val icon: androidx.compose.ui.graphics.vector.ImageVector,
)

private val NAV_TABS =
    listOf(
        NavTab("Play", ApgoIcons.Play),
        NavTab("Realms", ApgoIcons.Realms),
        NavTab("New Game", ApgoIcons.NewGame),
        NavTab("Activity", ApgoIcons.Activity),
        NavTab("Settings", ApgoIcons.Settings),
    )

/** The whole app: the setup flow until it is done, then the tabs with the dialogs that can open over them. */
@Composable
internal fun AppRoot(
    m: AppModel,
    modifier: Modifier = Modifier,
    backgroundPromptUp: Boolean = false,
) {
    if (m.setup.visible) {
        Surface(modifier.fillMaxSize()) { SetupFlow(m) }
    } else {
        // One root for the screen and its dialogs (dialogs open their own windows, so this adds nothing visible).
        Box(modifier) {
            val othersUp = listOf(m.scans.ask != null, m.yamlText != null, backgroundPromptUp)
            if (showHomeOffer(othersUp)) HomeWifiDialog(m.presence)
            // Back from any other tab goes to Play; the realm editor handles its own Back (to the list); on Play it leaves the app
            // as usual.
            BackHandler(enabled = m.tab != AppTab.PLAY) { m.tab = AppTab.PLAY }
            Scaffold(bottomBar = { AppNavigationBar(m) }) { pad -> AppBody(m, Modifier.padding(pad)) }
            ScanAskDialog(m)
            YamlDialog(m)
        }
    }
}

@Composable
private fun AppNavigationBar(m: AppModel) {
    // No pill behind the selected icon: the icon and label colour alone mark it.
    val colors =
        NavigationBarItemDefaults.colors(
            selectedIconColor = MaterialTheme.colorScheme.primary,
            selectedTextColor = MaterialTheme.colorScheme.primary,
            indicatorColor = Color.Transparent,
        )
    NavigationBar {
        NAV_TABS.forEachIndexed { i, tab ->
            NavigationBarItem(
                selected = m.tab == i,
                onClick = {
                    m.tab = i
                    if (i == AppTab.REALMS) m.editing = null // tapping Realms again leaves the editor
                },
                icon = { Icon(tab.icon, contentDescription = tab.label) },
                label = { Text(tab.label) },
                colors = colors,
            )
        }
    }
}

// What the app is busy with, its last message, and the tab on show.
@Composable
private fun AppBody(
    m: AppModel,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxSize()) {
        m.busy?.let {
            Text(it, Modifier.padding(horizontal = 12.dp, vertical = 4.dp), fontSize = 12.sp)
            m.scans.busyFraction?.let { f -> LinearProgressIndicator(progress = { f }, modifier = Modifier.fillMaxWidth()) }
                ?: LinearProgressIndicator(Modifier.fillMaxWidth())
        }
        if (m.status.isNotBlank()) {
            Text(
                m.status,
                Modifier.padding(horizontal = 12.dp, vertical = 2.dp),
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.primary,
            )
        }
        Box(Modifier.weight(1f)) {
            when (m.tab) {
                AppTab.REALMS -> RealmsScreen(m)
                AppTab.NEW_GAME -> NewGameScreen(m)
                AppTab.ACTIVITY -> ActivityScreen(m)
                AppTab.SETTINGS -> SettingsScreen(m)
                else -> PlayScreen(m)
            }
        }
    }
}

// Asks before a big download.
@Composable
private fun ScanAskDialog(m: AppModel) {
    val ask = m.scans.ask ?: return
    AlertDialog(
        onDismissRequest = { m.scans.ask = null },
        title = { Text("A big area") },
        text = {
            Text(
                "Finding places here needs about ${ask.requests} downloads (${ask.tiles} map areas) " +
                    "and may take a few minutes. It only has to be done once for each area.",
            )
        },
        confirmButton = {
            TextButton(onClick = {
                m.scans.ask = null
                m.scans.scan(ask.id, confirmed = true)
            }) { Text("Continue") }
        },
        dismissButton = { TextButton(onClick = { m.scans.ask = null }) { Text("Not now") } },
    )
}

// The Archipelago YAML that was just exported, with a Copy button.
@Composable
private fun YamlDialog(m: AppModel) {
    val clip = LocalClipboard.current
    val scope = rememberCoroutineScope() // before the early return: the dialog (and this scope) must outlive the copy
    val yaml = m.yamlText ?: return
    AlertDialog(
        onDismissRequest = { m.yamlText = null },
        title = { Text("Archipelago YAML") },
        text = { Text(yaml, fontSize = 11.sp, modifier = Modifier.verticalScroll(rememberScrollState())) },
        confirmButton = {
            TextButton(onClick = {
                scope.launch {
                    clip.setClipEntry(ClipEntry(ClipData.newPlainText("Archipelago YAML", yaml)))
                    m.status = "YAML copied"
                    m.yamlText = null
                }
            }) { Text("Copy") }
        },
        dismissButton = { TextButton(onClick = { m.yamlText = null }) { Text("Close") } },
    )
}
