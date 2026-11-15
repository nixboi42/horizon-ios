# Horizon iOS 0.2.0 build 11

Preview release for arm64 iPhone and iPad, targeting iOS and iPadOS 15.0. The unsigned IPA requires re-signing before installation. Prepare and import your own Minecraft: Java Edition 26.3 resources to play. See the [installation guide](overlay/INSTALL.md).

## Highlights

- Native launcher with isolated game instances and a shared import for prepared resources.
- Three touch schemes, customizable touch layouts, tap-to-interact, and hold-to-mine.
- Sprint and Crouch toggle on tap in the tap-to-interact schemes, and tap again to turn off.
- Native audio using imported sounds, plus improvements to startup and frame pacing.

## Downloads

- `HorizonIOS-0.2.0-11-touch-toggles-unsigned.ipa`
- `SHA256SUMS`
- [Build record](BUILD.json) and [packaging instructions](docs/build-11.md)

The original build 11 IPA has SHA-256 `a6f4c69e9007826d5fef8edfa5cebd6b6dc3467b977d8e193af510ea1b4b44e5`. It passed bundle metadata and release privacy checks. The local upload log records successful submission of build 11 to TestFlight. Physical-device acceptance of build 11, online-mode multiplayer, and long-session performance remain unverified. The [device validation notes](docs/validation.md) cover earlier builds.

Horizon iOS is an independent port. The release ships without Minecraft jars, game assets, or worlds.
