# Context Doc: Upstream Archipela-Go! App Architecture

_Last verified: 2026-10-07_

## Status

Upstream is semi-stale: last release 0.7.0 (2026-01-11), branch `archipela-go` at commit 125d11f. Open issues: #16 (slow generation), #17 (apworld manifest), #18 (saved connection edits lost), #19 (too few checks). Read-only reference for our clone.

## What This Is

React Native/Expo Android-first app (client for the Archipela-Go! apworld) at github.com/aki665/react-native-archipelago. Branches: `archipela-go` (the game; default branch on GitHub is the generic one), `text-client` (generic AP client, CONTRIBUTING says generic features target it), plus `apgo/*` feature branches (`apgo/overpass-migration`, `apgo/ban-locations`, ...).

## Environment

- Expo ~54.0.30, React Native 0.81.5, React 19.1.0, TypeScript ~5.9, pnpm 10.27 (+ patches), EAS builds (apk). Android package `com.aki665.archipelago`; version 0.7.0. APK ~108 MB (was 67.8 MB).
- Key deps: `archipelago.js ^2.0.4` (AP protocol), `react-native-maps ^1.26` (Google Maps on Android), `expo-location ~19` + `expo-task-manager` (geofencing), `expo-audio`, `expo-keep-awake`, `@react-native-async-storage/async-storage`, React Navigation 7 (native-stack + material-top-tabs), `@shopify/flash-list`, `@v3ron/react-native-circular-slider` (angle picker), `@coligo/react-native-table`, `@ungap/structured-clone` + `react-native-get-random-values` (polyfills for archipelago.js).
- `app.json`: plugins expo-location with `isAndroidBackgroundLocationEnabled`/iOS background; `usesCleartextTraffic: true` (ws:// servers); permission `ACCESS_BACKGROUND_LOCATION`; iOS just `supportsTablet` (iOS not shipped; README "later").
- `eas.json`: profiles development/preview/apk (+ apk2..apk4 experiments)/production. Script `pnpm buildApk` = `eas build -p android --profile apk`. EAS projectId is upstream's; do NOT reuse.
- `app.config.js` exists (not read; UNVERIFIED contents, 374 bytes).

## Structure and data flow

- `App.tsx`: providers `ErrorContext > ClientContext > SettingsContext`, native stack: `connect` (ConnectTabs), `connected`, `bannedLocations`. Calls `TaskManager.unregisterAllTasksAsync()` at startup.
- `ClientContext`: single `archipelago.js` `Client` + `connectionInfoRef` (url, name, game, options) for reconnects.
- `screens/ConnectTabs` -> `Connect.tsx` (host/port/slot/password, `client.login`, then asks to save under name `"{slot} @ {host}:{port}"`) and `SavedInfo.tsx` (list/edit/delete, connect). Saved connections live in AsyncStorage under the session name; derived keys `_trips`, `_tempTrips`, `_checked`, `_itemIndex` are suffixes (`EXTRA_DATA`); global keys `__settings`, `__bannedLocations`.
- `Connected.tsx`: top tabs Chat / Map (only after location permission) / Hints. Handles reconnect (auto retry count, "continue offline"), permission flow, chat PrintJSON parsing, keep-awake.
- `MapScreen.tsx` (27 KB, the core): loads/generates trips, renders markers (`APMarkers`), geofencing, item handling, goal, reroll, check sending.
  1. On mount: get position, `fetchSlotData`, `handleReconnect()` -> `getCoordinatesForLocations()`.
  2. If no `{session}_trips`: iterate `slot_data.trips` sorted by `key_needed`; per trip call `getLocations` (see location-generation doc); same angle `theta` per key group; saves partial progress to `_tempTrips`; then filters already-checked, saves `_trips`.
  3. Geofencing: `Location.startGeofencingAsync("apgo-geofencing", regions)` for trips with `receivedKeys >= key_needed`; task emits `locationEntered` via custom `LocationsEmitter` -> `handleGeofenceEnter` -> `checkedLocations` state -> `client.check([...])` and save `_checked`.
  4. Items: `handleItems` (utils) counts Keys/Collection Distance, strips macguffin letters from "Ap-Go!"/"Archipela-Go!"; popup + sound per new batch; `_itemIndex` tracks handled items.
  5. Goal: allsanity = no trips left; macguffin = string empty -> `updateStatus(goal)` + release/collect prompts.
- `components/LocationInfoPopup.tsx`: reroll (REROLL_TIME 120 s), ban, hint location/key, manual check (distance check vs MARKER_RADIUS unless cheat setting), `APInfoPopup` goal progress, `SettingsContext` (14 settings, persisted as one array under `__settings`).
- `utils/`: `getLocations.ts`, `handleItems.ts` (ID map, goal map), `storageHandler.ts` (AsyncStorage wrapper save/load/remove), `playAudio.ts`, `settingsHelpers.ts` (background permission prompts).
- Sounds: `assets/sounds` (rhodesmas CC-BY 3.0 for connected/disconnected; NewSoupVi ArchipelagoJingles MIT). `components/APLicense.tsx` shows licenses in-app.

## Permissions / background

- Foreground location always; background location only if `AUTOMATIC_SENDING` (default on) -- Android prompts via alert then `requestBackgroundPermissionsAsync`; denial disconnects. No foreground service/notification beyond expo-location geofencing. Geofences restart on app foreground, key receipt, trips change.

## Patches (pnpm `patchedDependencies`)

- `archipelago.js.patch`: (1) `new WebSocket(url.toString())` (RN WebSocket needs string, not URL object); (2) `check(...locations)` -> `check(locations: number[])` (call sites pass an array; upstream lib spread signature mismatch).
- `@coligo__react-native-table.patch` (66 KB): not read in detail (UNVERIFIED reason; likely React 19 / RN compat fixes). We can drop the table lib.

## License

- Repo `LICENSE`: MIT, (c) 2024 aki665, with a clause that subdirectories with their own LICENSE are exempt. We MAY copy/modify/sell code if the copyright + MIT notice are kept in copies/substantial portions. Sound assets: `assets/sounds/LICENSE.txt` (CC-BY 3.0 needs attribution; MIT jingles keep notice) -- or replace them. APWorld: no license file found; treat as all-rights-reserved-by-default until author (agilbert1412) confirms; reimplement logic rather than redistribute the zip. Archipelago core is MIT (`worlds/generic`). Game name/logo assets: avoid reusing icons/branding.

## Known bugs / limitations (file refs, our analysis)

- `utils/getLocations.ts`: slow/broken generation (see location-generation doc).
- `MapScreen.tsx`: `keyAmount = client.items.received.map(i => i.id===KEY).length` (2 places) = total items, not keys; `forEach(async ...)` re-generation of osmID "0" trips is fire-and-forget (not awaited before save/setTrips); `loc` is mutated from `location.coords` when home location used; `LocationsEmitter.on` dedupe check is wrong (`includes(events[0])`).
- Banned locations: query clause `way.a["id"!="N"]` (in `MapScreen.tsx`) filters on a nonexistent tag, so it matches every way -> widens the union instead of excluding; also only for osmIDs starting with "N" (nodes) but the clause is on ways.
- `SavedInfo.tsx` `saveEditedInfo` (issue #18): saves new `url/name/password` but keeps old `apInfo` object (`...editingValues`), and edit UI reads `editingValues.apInfo` -> port reverts.
- `SettingsContext.tsx`: `findSetting` returns an index (not value) when settings missing; `handleSettingChange` mutates state array in place; `MIN_RADIAN/MAX_RADIAN` global.
- `handleItems.ts`: traps are no-ops; macguffin string mutation; `Distance Reduction` id counted via COLLECTION_DISTANCE (wrong key for reductions) and never applied.
- Offline: no queue beyond `_checked`; issue #12 closed by "continue offline" option.
- Tests: none in repo (UNVERIFIED beyond tree listing); lint via eslint/prettier.

## Failed Approaches / Dead Ends

- Nominatim reverse geocoding (pre-0.7.0): returns only named roads; rural failures (#14); 1 req/s policy. Replaced by Overpass.

## Gotchas

- Expo `react-native-maps` on Android needs Google Maps API key in build config (UNVERIFIED how upstream supplies it; `app.config.js` may). Plan for this.
- `archipelago.js` v2 needs structuredClone polyfill and patches above.

## References

- <https://github.com/aki665/react-native-archipelago/tree/archipela-go>
- Related: `docs/context/archipela-go-game-design.md`, `docs/context/archipela-go-location-generation.md`
