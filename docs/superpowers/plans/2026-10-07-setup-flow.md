# Setup Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One guided setup (home pin, home Wi-Fi networks, car Bluetooth) that opens on first launch and from the Home card, replacing the hidden Presence screen.

**Architecture:** Two pure, JVM-tested helpers (`SetupProgress`, `WifiChoices`/`CarChoices`) hold all decisions. A new `SetupFlow` composable pages through three steps; step 1 reuses `HomePicker`, steps 2 and 3 take over the rows of `PresenceScreen`, which is deleted. `PresencePolicy` and `PresenceMonitor` do not change.

**Tech Stack:** Kotlin, Jetpack Compose, `WifiManager` scan results, `BluetoothAdapter.bondedDevices`, SharedPreferences, JUnit 4.

**Spec:** `docs/superpowers/specs/2026-10-07-setup-flow-design.md`

## Global Constraints

- Work on branch `feat/setup-flow`, never `main`.
- Conventional commits, imperative subject under 50 chars, body lines under 72 (the `committed` hook rejects otherwise). End each commit message with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Use the git MCP tools or the git CLI.
- TDD for the pure logic: failing test first, never edit a test to fit bad code. UI is checked on the emulator with screenshots (Task 8).
- Android build/test: `cd android && ./gradlew testDebugUnitTest --console=plain -q`; build the APK with `./gradlew assembleDebug --console=plain -q`.
- Matching in `PresenceSignals.isHome` is by SSID **or** BSSID: saving only an SSID covers every access point of that network.
- Android gives apps no list of saved Wi-Fi networks; nearby scan results are the search source.
- Wizard steps: 1 Home pin (required), 2 Home Wi-Fi (skippable), 3 Car Bluetooth (skippable). Setup away from home is allowed.
- Files live in `android/app/src/main/java/dev/apgo2/` (call it `$SRC`) and tests in `android/app/src/test/java/dev/apgo2/`.

## Review Focus

Failure modes the spec implies but no happy-path test covers. Each has a pinned test or a named manual check in the owning task.

1. Location permission denied or Wi-Fi off: the scan list is empty, the typed-name path still works, nothing crashes (Task 5 manual; empty-scan case in Task 2 tests).
2. Rescan throttled by Android: the player sees a message, not a silent no-op (Task 5 manual).
3. Hidden/blank/`<unknown ssid>` scan entries never appear as rows (Task 2 test).
4. Existing user with presence already configured, upgrading with `setupDone=false`: the wizard opens pre-filled, saved networks are ticked and nothing is lost (Task 2 test `savedNetworkOutOfRangeIsStillListedAndTicked`; Task 8 manual).
5. Bluetooth permission denied or no paired devices: a message shows and Finish still works; a saved car that is no longer paired stays listed so it can be removed (Task 2 test; Task 8 manual).

---

### Task 1: SetupProgress (pure)

**Files:**

- Create: `$SRC/presence/SetupProgress.kt`
- Test: `android/app/src/test/java/dev/apgo2/presence/SetupProgressTest.kt`

**Interfaces:**

- Produces: `enum class SetupStep { Home, Wifi, Car }`; `data class SetupProgress(homeSet: Boolean, wifiCount: Int, carCount: Int, setupDone: Boolean)` with `val missingWifi: Boolean`, `fun nextStep(): SetupStep?`, `fun needsAttention(): Boolean`.

- [ ] **Step 1: Write the failing test**

```kotlin
package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SetupProgressTest {
    private fun p(home: Boolean = false, wifi: Int = 0, car: Int = 0, done: Boolean = false) = SetupProgress(home, wifi, car, done)

    @Test fun emptyStartsAtHomeAndNeedsAttention() {
        assertEquals(SetupStep.Home, p().nextStep())
        assertTrue(p().needsAttention())
    }

    @Test fun homeOnlyPointsAtWifi() {
        assertEquals(SetupStep.Wifi, p(home = true).nextStep())
        assertTrue(p(home = true).missingWifi)
    }

    @Test fun homeAndWifiPointsAtCar() {
        assertEquals(SetupStep.Car, p(home = true, wifi = 1).nextStep())
    }

    @Test fun everythingDoneHasNoNextStepAndNoNag() {
        val all = p(home = true, wifi = 2, car = 1, done = true)
        assertNull(all.nextStep())
        assertFalse(all.needsAttention())
    }

    @Test fun skippedWifiStillNagsAfterSetupIsDone() {
        val skipped = p(home = true, wifi = 0, car = 0, done = true)
        assertTrue("home set but no Wi-Fi", skipped.needsAttention())
    }

    @Test fun skippedCarDoesNotNag() {
        assertFalse(p(home = true, wifi = 1, car = 0, done = true).needsAttention())
    }

    @Test fun notDoneNagsEvenWhenComplete() {
        assertTrue("an upgrading user who never saw the wizard", p(home = true, wifi = 1, car = 1, done = false).needsAttention())
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd android && ./gradlew testDebugUnitTest --tests 'dev.apgo2.presence.SetupProgressTest' --console=plain -q`
Expected: FAIL, compile error `Unresolved reference: SetupProgress`.

- [ ] **Step 3: Write the minimal implementation**

```kotlin
package dev.apgo2.presence

enum class SetupStep { Home, Wifi, Car }

/** What the player has set up so far. Pure: the Home card and the wizard both read it. */
data class SetupProgress(val homeSet: Boolean, val wifiCount: Int, val carCount: Int, val setupDone: Boolean) {
    /** Home is set but no home Wi-Fi is saved, so nothing can pause the game at home. */
    val missingWifi: Boolean get() = homeSet && wifiCount == 0

    /** First step that still has nothing in it, or null when all three have something. */
    fun nextStep(): SetupStep? = when {
        !homeSet -> SetupStep.Home
        wifiCount == 0 -> SetupStep.Wifi
        carCount == 0 -> SetupStep.Car
        else -> null
    }

    /** Show the "Finish setup" nag: the wizard was never finished, or home Wi-Fi is missing. A skipped car step is fine. */
    fun needsAttention(): Boolean = !setupDone || missingWifi
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: same command. Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/presence/SetupProgress.kt android/app/src/test/java/dev/apgo2/presence/SetupProgressTest.kt
git commit -m "feat: add SetupProgress for the setup flow"
```

---

### Task 2: Wi-Fi and car choice lists (pure)

**Files:**

- Create: `$SRC/presence/Choices.kt`
- Test: `android/app/src/test/java/dev/apgo2/presence/ChoicesTest.kt`

**Interfaces:**

- Consumes: `HomeNetwork(ssid, bssid)`, `WifiId(ssid, bssid)`, `CarDevice(name, address)`, `PresenceSignals.cleanSsid(String?)` (all in `presence/PresenceSignals.kt`).
- Produces: `data class WifiChoice(ssid: String, bssid: String?, saved: Boolean, connected: Boolean)`; `WifiChoices.merge(saved: List<HomeNetwork>, current: WifiId?, nearby: List<String>, query: String): List<WifiChoice>`; `CarChoices.merge(paired: List<CarDevice>, saved: List<CarDevice>, query: String): List<CarDevice>`.

- [ ] **Step 1: Write the failing test**

```kotlin
package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChoicesTest {
    private fun home(ssid: String, bssid: String? = null) = HomeNetwork(ssid, bssid)
    private fun ssids(l: List<WifiChoice>) = l.map { it.ssid }

    @Test fun connectedFirstThenSavedThenNearbyAlphabetical() {
        val r = WifiChoices.merge(listOf(home("Saved")), WifiId("\"Now\"", "aa:bb"), listOf("zeta", "Alpha"), "")
        assertEquals(listOf("Now", "Saved", "Alpha", "zeta"), ssids(r))
    }

    @Test fun sameNameIsListedOnce() {
        val r = WifiChoices.merge(listOf(home("Home")), WifiId("Home", "aa:bb"), listOf("Home", "Home", "Other"), "")
        assertEquals(listOf("Home", "Other"), ssids(r))
    }

    @Test fun hiddenAndUnknownNamesAreDropped() {
        val r = WifiChoices.merge(emptyList(), null, listOf("", "  ", "<unknown ssid>", "\"\"", "Real"), "")
        assertEquals(listOf("Real"), ssids(r))
    }

    @Test fun savedNetworkOutOfRangeIsStillListedAndTicked() {
        val r = WifiChoices.merge(listOf(home("Away5G")), null, emptyList(), "")
        assertEquals(1, r.size)
        assertTrue(r[0].saved)
        assertTrue(!r[0].connected)
    }

    @Test fun emptyScanAndNothingSavedIsAnEmptyList() {
        assertEquals(emptyList<WifiChoice>(), WifiChoices.merge(emptyList(), null, emptyList(), ""))
    }

    @Test fun queryFiltersCaseInsensitively() {
        val r = WifiChoices.merge(emptyList(), null, listOf("Kitchen-2G", "Kitchen-5G", "Neighbour"), "  kitchen ")
        assertEquals(listOf("Kitchen-2G", "Kitchen-5G"), ssids(r))
    }

    @Test fun queryThatMatchesNothingIsEmpty() {
        assertEquals(emptyList<WifiChoice>(), WifiChoices.merge(emptyList(), null, listOf("A"), "zzz"))
    }

    @Test fun onlyTheConnectedNetworkCarriesABssid() {
        val r = WifiChoices.merge(emptyList(), WifiId("Now", "aa:bb"), listOf("Other"), "")
        assertEquals("aa:bb", r[0].bssid)
        assertNull(r[1].bssid)
        assertTrue(r[0].connected)
    }

    @Test fun savedBssidIsKept() {
        assertEquals("cc:dd", WifiChoices.merge(listOf(home("Home", "cc:dd")), null, emptyList(), "")[0].bssid)
    }

    private fun car(name: String, addr: String) = CarDevice(name, addr)

    @Test fun pairedCarsComeFirstAndSavedUnpairedStayListed() {
        val r = CarChoices.merge(listOf(car("Buds", "A1")), listOf(car("Old car", "B2"), car("Buds", "a1")), "")
        assertEquals(listOf("Buds", "Old car"), r.map { it.name })
    }

    @Test fun carQueryFiltersByNameCaseInsensitively() {
        val r = CarChoices.merge(listOf(car("Honda Civic", "A"), car("Buds", "B")), emptyList(), "CIV")
        assertEquals(listOf("Honda Civic"), r.map { it.name })
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd android && ./gradlew testDebugUnitTest --tests 'dev.apgo2.presence.ChoicesTest' --console=plain -q`
Expected: FAIL, `Unresolved reference: WifiChoices`.

- [ ] **Step 3: Write the minimal implementation**

```kotlin
package dev.apgo2.presence

/** One row in the home Wi-Fi list. [saved] means it is ticked (a home network); [bssid] is only known for the connected or an already saved one. */
data class WifiChoice(val ssid: String, val bssid: String?, val saved: Boolean, val connected: Boolean)

/** What the setup wizard lists for home Wi-Fi. Pure. */
object WifiChoices {
    /** Connected network first, then saved, then nearby A-Z; one row per name; unusable names dropped; [query] filters by name, ignoring case. */
    fun merge(saved: List<HomeNetwork>, current: WifiId?, nearby: List<String>, query: String): List<WifiChoice> {
        val savedBySsid = saved.mapNotNull { h -> PresenceSignals.cleanSsid(h.ssid)?.let { it to h } }.toMap()
        val now = PresenceSignals.cleanSsid(current?.ssid)
        val ordered = LinkedHashSet<String>()
        now?.let { ordered += it }
        ordered += savedBySsid.keys
        ordered += nearby.mapNotNull { PresenceSignals.cleanSsid(it) }.distinct().sortedWith(String.CASE_INSENSITIVE_ORDER)
        val q = query.trim()
        return ordered.filter { q.isEmpty() || it.contains(q, ignoreCase = true) }.map { ssid ->
            WifiChoice(ssid, if (ssid == now) current?.bssid else savedBySsid[ssid]?.bssid, ssid in savedBySsid, ssid == now)
        }
    }
}

/** What the setup wizard lists for the car. Pure. */
object CarChoices {
    /** Paired devices first, then saved ones that are no longer paired (so they can still be removed), filtered by name. */
    fun merge(paired: List<CarDevice>, saved: List<CarDevice>, query: String): List<CarDevice> {
        val all = paired + saved.filterNot { s -> paired.any { it.address.equals(s.address, ignoreCase = true) } }
        val q = query.trim()
        return all.filter { q.isEmpty() || it.name.contains(q, ignoreCase = true) }
    }
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: same command. Expected: PASS (11 tests).

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/presence/Choices.kt android/app/src/test/java/dev/apgo2/presence/ChoicesTest.kt
git commit -m "feat: add Wi-Fi and car choice lists"
```

---

### Task 3: "Protection off" chip text

**Files:**

- Modify: `$SRC/presence/PresenceSignals.kt` (`PresenceText.chip`)
- Test: `android/app/src/test/java/dev/apgo2/presence/PresenceSignalsTest.kt:78`

This is a deliberate requirement change (spec "Nag"), not a test bent to fit code: with no home Wi-Fi and no car saved, the chip says so instead of a plain "Tracking".

- [ ] **Step 1: Change the expectation first**

In `PresenceSignalsTest.kt` line 78 change `"Tracking"` to `"Protection off"`:

```kotlin
        assertEquals("Protection off", PresenceText.chip(PresenceState.OutsideZones, configured = false))
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd android && ./gradlew testDebugUnitTest --tests 'dev.apgo2.presence.PresenceSignalsTest' --console=plain -q`
Expected: FAIL, `expected:<Protection off> but was:<Tracking>`.

- [ ] **Step 3: Implement**

In `PresenceText.chip` replace the doc comment and first branch:

```kotlin
    /** "Protection off" when no home network or car is saved, since nothing can pause the game then (the Home card offers the setup). */
    fun chip(state: PresenceState, configured: Boolean): String = if (!configured) "Protection off" else when (state) {
```

- [ ] **Step 4: Run it to verify it passes**

Run: same command. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/presence/PresenceSignals.kt android/app/src/test/java/dev/apgo2/presence/PresenceSignalsTest.kt
git commit -m "feat: say protection is off when unconfigured"
```

---

### Task 4: Platform plumbing (scanner, setupDone, model state)

**Files:**

- Create: `$SRC/presence/WifiScanner.kt`
- Modify: `android/app/src/main/AndroidManifest.xml`
- Modify: `$SRC/presence/PresenceSettings.kt`
- Modify: `$SRC/AppModel.kt:76` (replace `showPresence`)
- Modify: `docs/superpowers/specs/2026-10-07-setup-flow-design.md` (permission line)

**Interfaces:**

- Consumes: `SetupProgress`, `SetupStep` (Task 1).
- Produces: `WifiScanner(ctx).nearby(): List<String>`, `WifiScanner.rescan(): Boolean`; `PresenceSettings.setupDone: Boolean` (get/set); on `AppModel`: `showSetup: Boolean` (read only), `setupStart: SetupStep`, `setupProgress(): SetupProgress`, `openSetup(from: SetupStep? = null)`, `leaveSetup()`, `finishSetup()`.

No unit test: these wrap Android services and preferences; they are exercised in Task 8. Keep them thin.

- [ ] **Step 1: Create the scanner**

```kotlin
package dev.apgo2.presence

import android.annotation.SuppressLint
import android.content.Context
import android.net.wifi.WifiManager

/** Reads the Wi-Fi networks in range, to offer as home networks. Needs the location permission the app already asks for. */
class WifiScanner(ctx: Context) {
    private val wifi = ctx.applicationContext.getSystemService(WifiManager::class.java)

    /** Names from the last scan, raw (blank for hidden networks); empty without permission or with Wi-Fi off. Feed to [WifiChoices.merge]. */
    @Suppress("DEPRECATION")
    @SuppressLint("MissingPermission")
    fun nearby(): List<String> = runCatching { wifi?.scanResults?.map { it.SSID } ?: emptyList() }.getOrDefault(emptyList())

    /** Ask for a fresh scan; false when Android refused (throttled to about 4 per 2 minutes, Wi-Fi off, no permission). Results arrive later via [nearby]. */
    @Suppress("DEPRECATION")
    @SuppressLint("MissingPermission")
    fun rescan(): Boolean = runCatching { wifi?.startScan() == true }.getOrDefault(false)
}
```

- [ ] **Step 2: Manifest permission**

In `AndroidManifest.xml` after the `ACCESS_WIFI_STATE` line add:

```xml
    <uses-permission android:name="android.permission.CHANGE_WIFI_STATE" />
```

`startScan()` needs `CHANGE_WIFI_STATE` (a normal, install-time permission). Reading `scanResults` needs the `ACCESS_FINE_LOCATION` the app already holds. The spec also named `NEARBY_WIFI_DEVICES` for API 33+; fine location should be enough, so it is not added unless Task 8's phone check shows an empty list with permissions granted. Update the spec's permission sentence to match:

In `docs/superpowers/specs/2026-10-07-setup-flow-design.md` replace
`Needs the location permission the app already asks for\n     (plus \`NEARBY_WIFI_DEVICES\` on API 33+, declared \`neverForLocation\`).`
with
`Needs the location permission the app already asks for\n     and \`CHANGE_WIFI_STATE\` for rescans (add \`NEARBY_WIFI_DEVICES\` on API 33+ only if the phone check shows scans coming back empty).`

- [ ] **Step 3: `setupDone` in settings**

In `PresenceSettings.kt`, above `fun addHome`:

```kotlin
    /** True once the player finished or explicitly skipped the setup wizard; until then it opens on every start. */
    var setupDone: Boolean
        get() = prefs.getBoolean("setup_done", false)
        set(v) { prefs.edit().putBoolean("setup_done", v).apply() }
```

- [ ] **Step 4: Model state**

In `AppModel.kt` replace `var showPresence by mutableStateOf(false)` with:

```kotlin
    /** The setup wizard (home pin, home Wi-Fi, car Bluetooth) is open. It opens by itself on every start until it has been finished or skipped once. */
    var showSetup by mutableStateOf(!settings.setupDone)
        private set
    /** The step the wizard opens on. */
    var setupStart = SetupStep.Home
        private set

    fun setupProgress() = SetupProgress(home != null, settings.homeNetworks.size, settings.carDevices.size, settings.setupDone)
    fun openSetup(from: SetupStep? = null) { setupStart = from ?: SetupStep.Home; showSetup = true }
    /** Close without marking it done (Back on the first step): the Home card keeps offering it. */
    fun leaveSetup() { showSetup = false }
    fun finishSetup() { settings.setupDone = true; showSetup = false }
```

Add `import dev.apgo2.presence.SetupProgress` and `import dev.apgo2.presence.SetupStep` next to the other `dev.apgo2.presence` imports. (The code does not compile until Task 6 removes the `showPresence` uses; do Steps 1–4 and 5 together with Task 6, or stub nothing and commit at the end of Task 6.)

- [ ] **Step 5: Commit after Task 6 compiles** (see Task 6 Step 6).

---

### Task 5: Wizard steps 2 and 3 (Wi-Fi and car pages)

**Files:**

- Create: `$SRC/SetupSteps.kt`

**Interfaces:**

- Consumes: `WifiChoices`, `CarChoices`, `WifiScanner` (Tasks 2 and 4); `AppModel.settings`, `monitor.currentNetwork()`, `locationPermitted`, `evaluatePresence()`, `ensureMonitor(Boolean)`.
- Produces: `internal fun StepPage(...)`, `internal fun WifiStep(m, onBack, onNext)`, `internal fun CarStep(m, onBack, onDone)`.

- [ ] **Step 1: Write the file**

```kotlin
package dev.apgo2

import android.Manifest
import android.bluetooth.BluetoothManager
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.presence.CarChoices
import dev.apgo2.presence.CarDevice
import dev.apgo2.presence.HomeNetwork
import dev.apgo2.presence.PresenceSignals
import dev.apgo2.presence.WifiChoice
import dev.apgo2.presence.WifiChoices
import dev.apgo2.presence.WifiScanner
import kotlinx.coroutines.delay

/** The frame every text step shares: title, why it matters, a scrolling body, and Back / Skip / Next. */
@Composable
internal fun StepPage(title: String, why: String, next: String, skip: String?, onBack: () -> Unit, onNext: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    BackHandler { onBack() }
    Column(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(title, style = MaterialTheme.typography.titleLarge)
        Text(why, fontSize = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp), content = content)
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onBack) { Text("Back") }
            Spacer(Modifier.weight(1f))
            skip?.let { TextButton(onClick = onNext) { Text(it) } }
            Button(onClick = onNext) { Text(next) }
        }
    }
}

/** Step 2: tick every Wi-Fi network your home uses. Saving a name covers every access point on it. */
@Composable
internal fun WifiStep(m: AppModel, onBack: () -> Unit, onNext: () -> Unit) {
    val ctx = LocalContext.current
    val scanner = remember { WifiScanner(ctx) }
    var saved by remember { mutableStateOf(m.settings.homeNetworks) }
    var nearby by remember { mutableStateOf(scanner.nearby()) }
    var query by remember { mutableStateOf("") }
    var typed by remember { mutableStateOf("") }
    var note by remember { mutableStateOf<String?>(null) }
    var scanning by remember { mutableStateOf(false) }
    // The scan answers a moment later; read it then.
    LaunchedEffect(scanning) { if (scanning) { delay(3_000); nearby = scanner.nearby(); scanning = false } }
    LaunchedEffect(Unit) { if (m.locationPermitted && scanner.rescan()) scanning = true }

    val inRange = nearby.mapNotNull { PresenceSignals.cleanSsid(it) }.toSet()
    val choices = WifiChoices.merge(saved, m.monitor.currentNetwork(), nearby, query)
    fun toggle(c: WifiChoice, on: Boolean) {
        if (on) m.settings.addHome(HomeNetwork(c.ssid, c.bssid)) else m.settings.removeHome(c.ssid)
        saved = m.settings.homeNetworks
        m.evaluatePresence()
    }
    fun addTyped() {
        val ssid = PresenceSignals.cleanSsid(typed) ?: return
        m.settings.addHome(HomeNetwork(ssid, null))
        saved = m.settings.homeNetworks
        typed = ""
        m.evaluatePresence()
    }

    StepPage(
        title = "Home Wi-Fi · step 2 of 3",
        why = "While you are on any of these networks nothing counts and GPS turns off. That saves battery and stops cheating. Tick every network your home uses.",
        next = "Next", skip = if (saved.isEmpty()) "Skip, I'll do this at home" else null, onBack = onBack, onNext = onNext,
    ) {
        OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth(), label = { Text("Search networks") }, singleLine = true)
        if (choices.isEmpty()) {
            Text(
                if (query.isBlank()) "No networks found. Connect to your home Wi-Fi, or type its name below." else "Nothing matches \"$query\".",
                fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        choices.forEach { c ->
            Row(Modifier.fillMaxWidth().clickable { toggle(c, !c.saved) }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(c.saved, { toggle(c, it) })
                Column {
                    Text(c.ssid)
                    val tag = when { c.connected -> "Connected now"; c.saved && c.ssid !in inRange -> "Saved, not in range"; else -> null }
                    tag?.let { Text(it, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
            }
        }
        if (!m.locationPermitted) Text("Allow location to list nearby networks.", fontSize = 12.sp, color = MaterialTheme.colorScheme.error)
        note?.let { Text(it, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        OutlinedButton(enabled = !scanning, onClick = {
            if (scanner.rescan()) { scanning = true; note = null } else note = "Android limits how often Wi-Fi can be scanned. Showing the last results."
        }) { Text(if (scanning) "Scanning…" else "Rescan") }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(typed, { typed = it }, Modifier.weight(1f), label = { Text("Add a network by name") }, singleLine = true)
            OutlinedButton(enabled = PresenceSignals.cleanSsid(typed) != null, onClick = { addTyped() }) { Text("Add") }
        }
    }
}

/** Step 3: tick the Bluetooth device that is your car. Optional. */
@Composable
internal fun CarStep(m: AppModel, onBack: () -> Unit, onDone: () -> Unit) {
    val ctx = LocalContext.current
    var car by remember { mutableStateOf(m.settings.carDevices) }
    var query by remember { mutableStateOf("") }
    var btOk by remember { mutableStateOf(Build.VERSION.SDK_INT < 31 || ctx.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED) }
    // Tell the model too: it restarts the monitor so car detection works without leaving the app.
    val askBt = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { btOk = it; m.ensureMonitor(it) }
    val paired = remember(btOk) {
        if (!btOk) emptyList()
        else runCatching { ctx.getSystemService(BluetoothManager::class.java)?.adapter?.bondedDevices?.map { CarDevice(it.name ?: it.address, it.address) } ?: emptyList() }.getOrDefault(emptyList())
    }
    fun toggle(d: CarDevice, on: Boolean) {
        val now = m.settings.carDevices.filterNot { it.address == d.address }
        m.settings.setCar(if (on) now + d else now)
        car = m.settings.carDevices
        m.evaluatePresence()
    }
    StepPage(
        title = "Car Bluetooth · step 3 of 3",
        why = "While your car is connected nothing counts, so rides do not turn into pickups. Skip this if you never drive while playing.",
        next = "Finish", skip = if (car.isEmpty()) "Skip" else null, onBack = onBack, onNext = onDone,
    ) {
        if (!btOk) OutlinedButton(onClick = { askBt.launch(Manifest.permission.BLUETOOTH_CONNECT) }) { Text("Allow Bluetooth to pick your car") }
        else if (paired.isEmpty() && car.isEmpty()) Text("No paired Bluetooth devices found. Pair your car in the phone's Bluetooth settings first.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth(), label = { Text("Search devices") }, singleLine = true)
        CarChoices.merge(paired, car, query).forEach { d ->
            val on = car.any { it.address == d.address }
            Row(Modifier.fillMaxWidth().clickable { toggle(d, !on) }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(on, { toggle(d, it) })
                Text(d.name)
            }
        }
    }
}
```

- [ ] **Step 2: Do not commit yet.** Task 6 wires the files so the build passes; commit both together.

---

### Task 6: Wizard shell, HomePicker reuse, remove PresenceScreen

**Files:**

- Create: `$SRC/SetupFlow.kt`
- Modify: `$SRC/Screens.kt` (`AppRoot` line ~128, `RealmsScreen` line ~183, `HomeCard` ~234-262, `HomePicker` ~304-352)
- Delete: `$SRC/PresenceScreen.kt`

**Interfaces:**

- Consumes: `WifiStep`, `CarStep` (Task 5); `AppModel.setupStart`, `finishSetup()`, `leaveSetup()`, `openSetup()`, `setupProgress()` (Task 4).
- Produces: `fun SetupFlow(m: AppModel)`; `HomePicker(m, onBack, onConfirm = onBack, title = "Home", confirmLabel = "Done", requireHome = false)` now `internal`.

- [ ] **Step 1: The shell**

```kotlin
package dev.apgo2

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import dev.apgo2.presence.SetupStep

/** First-run and edit wizard: where home is, which Wi-Fi networks are home, which Bluetooth device is the car. */
@Composable
fun SetupFlow(m: AppModel) {
    var step by remember { mutableStateOf(m.setupStart) }
    when (step) {
        SetupStep.Home -> HomePicker(m, title = "Home · step 1 of 3", confirmLabel = "Next", requireHome = true, onBack = { m.leaveSetup() }, onConfirm = { step = SetupStep.Wifi })
        SetupStep.Wifi -> WifiStep(m, onBack = { step = SetupStep.Home }, onNext = { step = SetupStep.Car })
        SetupStep.Car -> CarStep(m, onBack = { step = SetupStep.Wifi }, onDone = { m.finishSetup() })
    }
}
```

- [ ] **Step 2: Make `HomePicker` reusable** (in `Screens.kt`)

Change the signature and the three spots that used `onClose`/`"Home"`/`"Done"`:

```kotlin
@Composable
internal fun HomePicker(m: AppModel, onBack: () -> Unit, onConfirm: () -> Unit = onBack, title: String = "Home", confirmLabel: String = "Done", requireHome: Boolean = false) {
```

- `BackHandler { onClose() }` becomes `BackHandler { onBack() }`.
- The top-right button becomes `Button(onClick = onConfirm, enabled = !requireHome || saved, contentPadding = ...) { IconLabel(confirmLabel, ApgoIcons.Done, 14.sp) }`. (`saved` is true once a pin was placed or home already existed, so Next stays disabled until home is really saved.)
- `Text("Home", style = MaterialTheme.typography.titleSmall)` becomes `Text(title, style = MaterialTheme.typography.titleSmall)`.
- In `RealmsScreen`: `if (m.pickingHome) { HomePicker(m, onBack = { m.pickingHome = false }); return }`.

- [ ] **Step 3: Open the wizard from the root**

In `AppRoot` replace the `showPresence` line with:

```kotlin
    if (m.showSetup) { Surface(Modifier.fillMaxSize()) { SetupFlow(m) }; return }
```

- [ ] **Step 4: Home card button and nag**

In `HomeCard`, add `val progress = m.setupProgress()` after `val home = ...`, and replace `OutlinedButton(onClick = { m.showPresence = true }) { Text("Presence") }` with:

```kotlin
                if (progress.missingWifi) FeedbackText("Add your home Wi-Fi so the game pauses at home", Tone.Warning)
                OutlinedButton(onClick = { m.openSetup(if (progress.needsAttention()) progress.nextStep() else null) }) { Text(if (progress.needsAttention()) "Finish setup" else "Setup") }
```

Add `import dev.apgo2.ui.FeedbackText` and `import dev.apgo2.ui.Tone` to `Screens.kt` if not already imported (they live in `dev.apgo2.ui`).

- [ ] **Step 5: Delete the old screen and compile**

```bash
git rm android/app/src/main/java/dev/apgo2/PresenceScreen.kt
cd android && ./gradlew assembleDebug testDebugUnitTest --console=plain -q
```

Expected: builds; all unit tests pass. Fix any import the compiler reports (`SetupStep` and `SetupProgress` in `AppModel.kt`; `Spacer`, `ColumnScope` in the step file).

- [ ] **Step 6: Commit Tasks 4-6 together**

```bash
git add -A android docs/superpowers/specs/2026-10-07-setup-flow-design.md
git commit -m "feat: guide home, Wi-Fi and car in a setup flow" -m "Replaces the Presence screen with a three-step wizard that opens on first launch and from the Home card."
```

---

### Task 7: Docs and backlog

**Files:**

- Modify: `docs/context/v1-architecture-and-status.md` (Presence section, lines ~55-60)
- Modify: `docs/context/outdoor-test-plan.md` (any mention of the Presence screen)

- [ ] **Step 1: Find stale mentions**

Run: `grep -n -i "presence screen\|Presence button\|PresenceScreen" -r docs CLAUDE.md`

- [ ] **Step 2: Update them**

In the Presence section of `v1-architecture-and-status.md` replace the mention of the Presence screen with: setup lives in `SetupFlow` (steps in `SetupSteps.kt`, pure helpers `SetupProgress` and `WifiChoices`/`CarChoices` in `presence/`); `PresenceSettings.setupDone` gates the first-run wizard; the Home card shows "Finish setup"; the Play chip reads "Protection off" when nothing is configured; Wi-Fi choices come from nearby scan results because Android exposes no saved-network list. Fix any outdoor-test-plan step that says to open the Presence screen to say "Home card, Setup".

- [ ] **Step 3: File the follow-up issue**

```bash
gh issue create --title "Offer to add home Wi-Fi when the phone arrives home" --label enhancement --body "Setup flow lets the player skip Wi-Fi when away. When GPS shows the phone near the saved home pin and no home Wi-Fi is saved, prompt: 'You're home: add this Wi-Fi?'. Out of scope of the setup-flow spec."
```

- [ ] **Step 4: Commit**

```bash
git add docs
git commit -m "docs: describe the setup flow"
```

---

### Task 8: Emulator and phone verification (manual, with screenshots)

**Files:** none (read-only checks; fix anything found in a follow-up commit with its own test if it is logic).

- [ ] **Step 1: Install on the emulator**

```bash
just emu-start
adb -s emulator-5554 install -r android/app/build/outputs/apk/debug/app-debug.apk
adb -s emulator-5554 shell pm clear dev.apgo2   # fresh first run
adb -s emulator-5554 shell am start -n dev.apgo2/.MainActivity
```

Grant the permission dialogs. Expected: the wizard opens at step 1 (map) instead of the realm list, "Next" disabled until a pin is placed.

- [ ] **Step 2: Walk the steps and screenshot each** (`adb exec-out screencap -p > /tmp/x.png`, then Read it)
  - Step 1: tap the map, "Home saved" shows, Next enables.
  - Step 2: the emulator's `AndroidWifi` appears as "Connected now"; tick it; type a made-up name and Add; search box filters; Rescan twice quickly shows the throttle note; leaving it unticked shows "Skip, I'll do this at home".
  - Step 3: with no paired devices the message shows and Finish works.
  - After Finish: the app is on the Realms list; relaunch the app and the wizard does not return.
- [ ] **Step 3: Nag states** — clear Wi-Fi from the Home card flow (re-open Setup, untick, Finish): the Home card shows "Add your home Wi-Fi so the game pauses at home" and "Finish setup"; start a game and the Play chip reads "Protection off".
- [ ] **Step 4: Upgrade case** — with the previous build's saved Wi-Fi (or `adb shell run-as dev.apgo2` prefs edited so `setup_done` is absent), launch: the wizard opens pre-filled and saved networks are ticked.
- [ ] **Step 5: Phone check** — install on the Pixel 8 Pro (`ANDROID_SERIAL=10.0.20.151:40843`), open Setup at home, confirm all home SSIDs appear in the scan list. If the list is empty, first check that `ACCESS_FINE_LOCATION` is granted and the system location toggle is on. If it is still empty on API 33+, add `<uses-permission android:name="android.permission.NEARBY_WIFI_DEVICES" />` (request it at runtime) WITHOUT `neverForLocation`, which would make Android strip location-derived data such as SSIDs from scan results, and re-check.
- [ ] **Step 6: Report honestly** which of these were verified on the emulator only and which on the phone.

---

## Self-review

- **Spec coverage:** wizard + first run (Tasks 4, 6); `setupDone` (4); `SetupProgress` (1); step 1 reuse (6); Wi-Fi list with saved/current/nearby/typed/skip and search (2, 5); Rescan throttle message (5); car list with search and permission (2, 5); Home card badge/button (6); "Protection off" chip (3); `PresenceScreen` removed (6); testing (1, 2, 3, 8); docs and out-of-scope issue (7). Permission wording in the spec is corrected in Task 4 (`CHANGE_WIFI_STATE`; `NEARBY_WIFI_DEVICES` only if the phone check needs it).
- **Placeholders:** none; every code step has code.
- **Type consistency:** `SetupStep`, `SetupProgress.missingWifi/nextStep/needsAttention`, `WifiChoice.saved/connected/bssid`, `WifiChoices.merge`, `CarChoices.merge`, `WifiScanner.nearby/rescan`, `AppModel.showSetup/setupStart/openSetup/leaveSetup/finishSetup/setupProgress`, `HomePicker(onBack, onConfirm, title, confirmLabel, requireHome)` are used identically across tasks.
