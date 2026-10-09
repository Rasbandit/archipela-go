# Archipela-Go 2

An [Archipelago](https://archipelago.gg) multiworld game where checks are real-world places you reach by walking.
Your items unlock zones, tools and quests; your finds send items to the other players in the multiworld.

Android first, iOS later. Successor in spirit to [Archipela-Go!](https://github.com/aki665/react-native-archipelago),
written from scratch.

## Layout

| Path | What |
| -- | -- |
| `apworld/` | The Archipelago world (Python), game `Archipela-Go 2: Electric Boogaloo` |
| `core/` | Rust core: AP protocol, location generation, geofence, storage; exposed to apps via UniFFI |
| `android/` | Kotlin/Compose Android app |
| `docs/` | Specs, plans and context docs |

## Building

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup. In short: `just check` runs every gate, `just build` produces
`dist/ap_go2.apworld`, and `just android-run` installs the app on a connected phone.

## Licensing

| Path | Licence |
| -- | -- |
| `android/` | [PolyForm Noncommercial 1.0.0](android/LICENSE): free to read, build and change, not to sell |
| everything else (`apworld/`, `core/`, `docs/`, `scripts/`) | [MIT](LICENSE) |
| `core/vendor/` and other third-party code | its own licence, see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) |
