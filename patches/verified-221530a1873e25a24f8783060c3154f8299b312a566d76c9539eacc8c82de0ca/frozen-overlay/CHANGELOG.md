# Changelog

All notable changes to the iOS port are recorded here. This project is a port, so gameplay and
world-format changes are tracked upstream at
[horizon-rust/horizon](https://github.com/horizon-rust/horizon).

## 0.1.0-experimental — 2026-11-08

First public experimental release. This is a source and re-signable-IPA release, not an App Store
build.

### Added

- Native iOS host for Horizon's client, integrated server and rendering stack (`crates/horizon-ios`,
  `ios/`).
- wgpu/Metal rendering through `winit` and UIKit, landscape-only, iOS 16.0 minimum, arm64.
- Real single-player gameplay: bundled Creative world, Overworld generation, lighting, meshing,
  block breaking/placement, inventory, saving and relaunch.
- Direct-address multiplayer over the upstream 26.3 protocol.
- Touch controls for movement, look, jump, break, place, pick block, inventory, pause and chat.
- Native iOS text entry for server addresses, chat, signs, books, dialogs and world options, with
  IPv6/port handling and smart-punctuation disabled.
- Background save/flush request within iOS background time.
- Resource import from the app's Documents folder (`HorizonResources`), in addition to a
  developer-staged bundle, with per-file SHA-256 verification and actionable missing-resource
  messages.
- Release tooling: `build-app.sh`, `make-ipa.sh`, `verify-app.sh`, `check_release.py`, and a macOS
  CI workflow that compiles and packages without Minecraft data.
- First-run files: README, INSTALL, BUILDING, KNOWN_ISSUES, RELEASE_NOTES, NOTICE, CONTRIBUTING;
  architecture, validation and server-compatibility notes under `docs/`.

### Changed

- App display name is **Horizon iOS**; default bundle identifier is `dev.horizon.ios`.
- App version is `0.1.0` (`CFBundleVersion` 1).
- The first launch uses conservative render settings (distance 2, simulation 5, 60 FPS cap);
  subsequent launches respect the player's Video Settings instead of resetting them.
- The iOS UI string that reports missing resources now explains where to copy them.

### Fixed

- iOS compilation of clipboard, process-memory, protocol, input, renderer and HDR paths.
- Surface suspension/resume and the UIKit scene attachment on current iOS.
- Native text focus across background/resume and screen changes.
- Multiplayer readiness and touch routing when the integrated server is not owned by the client.
- Server-issued "Invalid packet" disconnects are reported as a server disconnect, not a local codec
  error (upstream protocol diagnostics).
- Release builds remap workspace, Cargo registry and home paths, and the privacy gate scans the
  executable so the builder's user name is not embedded in panic locations.
- The IPA packager strips private prepared resources by default and no longer relies on a stale
  bundle subfolder path.

### Validation

- Rust host tests for `horizon-ios`, resource-import and release privacy fixtures.
- Release builds for arm64 device and arm64 simulator; automated simulator UI and gameplay checks;
  physical iPhone 13 mini interaction checks. See [docs/ios-validation.md](docs/ios-validation.md)
  for the exact scope and the gaps.

### Not included

- Minecraft game files or assets, worlds, Microsoft account credentials, Apple signing material.
- App Store distribution, a launcher, audio, controllers, Realms or mod installation.
