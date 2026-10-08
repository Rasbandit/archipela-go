# Setup flow: home, home Wi-Fi, car Bluetooth

## Goal
Presence detection (home Wi-Fi, car Bluetooth) saves battery and stops cheating, but it lives on a hidden `PresenceScreen` behind a small button. Make it part
of one guided setup: shown on first launch, re-openable from the Home card to edit. UX only; `PresencePolicy` and `PresenceMonitor` do not change.

## Decisions (owner)
- One wizard, reused for first-run and edit. Steps: 1 Home pin, 2 Home Wi-Fi, 3 Car Bluetooth.
- Setting up away from home is allowed. Wi-Fi and car steps can be skipped; the app nags until Wi-Fi is added.
- Auto-detecting "you are home now" is out of scope (file an issue later).

## State
- `PresenceSettings.setupDone` (SharedPreferences `presence`): true once the wizard is finished or explicitly skipped. First launch shows the wizard when
  `!setupDone`.
- Pure `SetupProgress(homeSet, wifiCount, carCount, setupDone)` with `nextStep()` (first incomplete of Home, Wi-Fi, Car; Wi-Fi counts as incomplete only
  when none saved) and `needsAttention()` (home set but no Wi-Fi saved, or `!setupDone`). Unit-tested on the JVM, failing tests first.

## Steps
1. **Home pin.** Today's `HomePicker` as a step body; "Next" instead of "Done". Required: no home, no distances.
2. **Home Wi-Fi.** Copy: while on this Wi-Fi the app pauses and GPS turns off (battery, no cheating). Shows the current SSID with "Use this network".
   Choices, all writing `HomeNetwork(ssid, bssid?)`:
   - Current connection (captures SSID and BSSID).
   - Type the name by hand (SSID only, no BSSID). The way to set it up while away.
   - Skip ("Do this when you're home").
   Android gives apps no list of saved Wi-Fi networks (`getConfiguredNetworks()` returns empty for apps targeting API 29+), so there is no searchable
   "saved networks" picker. A nearby-scan list was considered and dropped: at the moment of setup the home network is only in range when at home, where
   "current connection" already works.
3. **Car Bluetooth.** Paired-device checklist (`BluetoothAdapter.bondedDevices`, needs `BLUETOOTH_CONNECT` on API 31+; permission ask inline) plus a
   search box filtering paired devices by name. Optional.

## Nag
- Home card: "Finish setup" badge when `needsAttention()`; tapping opens the wizard at `nextStep()`.
- Play presence chip: "Protection off" when no home Wi-Fi saved and no car device.
- The Home card's "Presence" button becomes "Setup" into the wizard. `PresenceScreen` is removed; its rows move into steps 2 and 3.

## Testing
- JVM: `SetupProgressTest` (empty, home only, home+wifi, skipped, all done, setupDone gating).
- Emulator screenshots for each step, first-run and edit, with the app not on Wi-Fi (typed SSID path) and on Wi-Fi.

## Out of scope
Auto-detect arrival at home; scanning nearby networks; changes to the presence policy or monitor.
