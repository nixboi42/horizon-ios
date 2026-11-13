# Horizon iOS `v0.1.0-experimental`

First public experimental release of the native iPhone and iPad port of
[Horizon](https://github.com/horizon-rust/horizon), a Minecraft: Java Edition 26.3 rewrite in Rust.

> **Unofficial project.** Horizon iOS is not an official Minecraft product, is not approved by or
> associated with Mojang or Microsoft, and is not affiliated with the Horizon project or its
> maintainers. Minecraft is a trademark of Mojang Synergies AB. You must own Minecraft: Java
> Edition 26.3 to prepare and use the game data.

This is a **source and unsigned-IPA release**, not an App Store build. It is intended for
developers and experienced sideloaders. The app contains no Minecraft code or assets and no Apple
signing material. There is no launcher and no Microsoft sign-in.

## Artifacts

| File | Size | SHA-256 |
| --- | --- | --- |
| `HorizonIOS-0.1.0-unsigned.ipa` | 29.3 MB (exact size below) | `1b8fa2bb5c0a43030a3fe53ff83395aa6bd7a13dec6c26d8e1eccf6a4c86b1c8` |
| Source | upstream base `18840db9` plus the iOS port changes | release commit (planned tag `v0.1.0-experimental`) |

The IPA is a standard unsigned app bundle: arm64, minimum iOS 16.0, iPhone and iPad, display name
**Horizon iOS**, bundle identifier `dev.horizon.ios`, version `0.1.0` (build `1`). It contains the
executable, `Info.plist`, `PkgInfo` and the English strings file — no `PreparedResources`, JARs,
world saves, provisioning profile, signature or credentials.

Verify the download:

```sh
shasum -a 256 HorizonIOS-0.1.0-unsigned.ipa
unzip -l HorizonIOS-0.1.0-unsigned.ipa
```

## Requirements

- An arm64 iPhone or iPad on **iOS/iPadOS 16.0 or later**.
- A Mac with **Minecraft: Java Edition 26.3** installed and played once, used to prepare your own
  resource files. The app never downloads Minecraft, and this release contains none of its data.
- A re-signing tool that accepts your own Apple ID or development team (Sideloadly, AltStore,
  Xcode, or a paid developer account). Free Apple ID signatures expire after seven days.
- Roughly 1.5 GB free on the device for the imported resources and the app's verified copy.

Building from source additionally needs an Apple silicon Mac with full Xcode and Rust. See
[BUILDING.md](BUILDING.md).

## Install

1. Re-sign the IPA with your own identity and install it, or build the app from source. The full
   walkthrough is in [INSTALL.md](INSTALL.md).
2. Prepare your own 26.3 data on the Mac with `horizon-prepare` and `ios/scripts/pack-resources.py`,
   producing a `HorizonResources` folder ([BUILDING.md](BUILDING.md)).
3. Copy that folder to `Documents/HorizonResources` in the app's container using Finder file
   sharing or the Files app, then relaunch. The app verifies every file against the SHA-256 index
   before opening a world.
4. The first launch opens a bundled Creative world. The default render distance is 2 chunks,
   simulation distance 5 and the frame cap 60. Raise them in **Video Settings** if your device can
   afford it; later launches keep your choices.

## What works

- Single-player gameplay on Horizon's integrated server: Overworld generation, lighting, meshing,
  block breaking and placement, inventory, saving and relaunch.
- Direct-address multiplayer over the upstream 26.3 protocol, including IPv6 and explicit ports.
  Offline-mode servers are the tested path; online-mode login from iOS is unvalidated.
- Touch controls for movement, look, jump, break, place, pick block, inventory, pause and chat.
- Native iOS keyboard editing for server addresses, chat, signs, books, dialogs and world options.
- Background save/flush within iOS's bounded background time.
- Resource import from Documents with per-file verification and actionable missing-resource
  messages.

## Tested hardware

| Device | OS | Scope |
| --- | --- | --- |
| iPhone 13 mini (A15) | iOS 27.2 | Earlier signed Release build: integrated-server gameplay, touch controls, native keyboard, background save, relaunch |
| iPad 5 (`iPad6,11`, A9, 2 GB) | iPadOS 16.7.16 | Older validation build: gameplay and performance observations, not this exact tree |
| iPhone 17 Pro simulator on Apple silicon | iOS 27.0 | Automated UI and gameplay checks against the development build |

The exact scope, commands and gaps are in [docs/ios-validation.md](docs/ios-validation.md). The
physical-device runs predate the final release-only changes (Documents resource import and
first-run settings persistence); this exact unsigned IPA was built and verified on the host, not
installed on a device.

## Verification for this release

Run on 2026-11-08 with Xcode 27.2 / iPhoneOS SDK 27.2 and Rust 1.99.1:

- `cargo fmt --all -- --check` — passed.
- `cargo test --locked -p horizon-ios --lib` — 14 passed, 1 ignored (requires private resources).
- `cargo clippy --locked -p horizon-ios --all-targets -- -D warnings` — passed.
- `cargo clippy --locked -p horizon-ios --lib --target aarch64-apple-ios -- -D warnings` — passed.
- Python release-privacy tests — 7 passed; `ios/scripts/check_release.py` — passed.
- Simulator Release build and device Release build — passed.
- `ios/scripts/verify-app.sh` on the extracted IPA payload — passed (bundle, version, iOS 16.0,
  arm64, no x86_64 slice, no provisioning profile or embedded resources).
- `ios/scripts/check_release.py --archive` and `--tree` on the payload — passed, including a byte
  scan of the executable for absolute home paths.
- `codesign -dv` reports the payload is not signed, as intended.
- The extracted executable reports `minos 16.0` and `sdk 27.2`.

Not verified for this exact artifact: installation on physical hardware, full simulator gameplay
and UI tests, online-mode authentication, long-session thermals and memory pressure.

## Known limitations

See [KNOWN_ISSUES.md](KNOWN_ISSUES.md). The main ones:

- No App Store or notarized build; the IPA must be re-signed by the user.
- Resources are manual; the IPA contains no Minecraft data.
- Online-mode multiplayer is unvalidated.
- Performance is not characterised; older 2 GB devices can be evicted under memory pressure.
- No audio output and no controller support.
- Landscape-only and full-screen; no iPad multitasking.
- Some older GrimAC/PacketEvents server builds kick 26.3 clients. This reproduces with vanilla
  clients and is fixed by updating the plugin; see
  [docs/ios-server-compatibility.md](docs/ios-server-compatibility.md).

## License and attribution

Horizon and this port are MIT-licensed. The upstream copyright notice is preserved in
[LICENSE](LICENSE); dependency licenses are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
and [NOTICE](NOTICE). Gameplay fixes belong upstream at
[horizon-rust/horizon](https://github.com/horizon-rust/horizon).
