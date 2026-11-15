# Horizon iOS

Horizon iOS is an unofficial native iPhone and iPad port of [Horizon](https://github.com/horizon-rust/horizon), a Rust rewrite of Minecraft: Java Edition 26.3. It runs on arm64 devices and renders with Metal.

**Latest preview · 0.2.0 build 11** · [Download the unsigned IPA](https://github.com/nixboi42/horizon-ios/releases/tag/v0.2.0-beta.11) · [Release notes](RELEASE_NOTES.md)

![Survival gameplay with the touch controls, health, hunger, and hotbar visible](docs/screenshots/survival-build-11.png)

Survival mode in Horizon iOS 0.2.0 build 11, captured on an iPhone 17 Pro simulator.

## Install and play

1. Download the unsigned build 11 IPA and re-sign it with your own Apple ID or development team. Follow the [installation guide](overlay/INSTALL.md) for sideloading and updates.
2. On a Mac, use your own Minecraft: Java Edition 26.3 installation to [prepare the game resources](overlay/BUILDING.md). The IPA ships without Minecraft jars, game assets, or worlds.
3. In Horizon iOS, choose **Import Prepared Resources**, select the `HorizonResources` folder in Files, then create or open an instance.

The app targets iOS and iPadOS 15.0. See the [device validation notes](docs/validation.md) for tested hardware. Build 11 has no recorded physical-device acceptance run.

## What’s in build 11

- A native launcher with isolated instances, a shared resource import, and offline profiles.
- Integrated single player with persistent worlds and direct-address multiplayer for compatible servers. Offline-mode servers are the validated path.
- Three touch schemes, customizable layouts, tap-to-interact controls, hold-to-mine, and Sprint and Crouch toggles. See [touch controls](docs/touch-controls.md).
- The iOS keyboard for text entry and native audio using imported sounds.

Microsoft sign-in and controller support are unavailable. See the [release notes](RELEASE_NOTES.md) for the validation status.

## Build from source

On an Apple silicon Mac with Xcode, XcodeGen, Git, Python 3.11 or later, and Rust:

```sh
rustup target add aarch64-apple-ios
./build.sh --locked --check
./build.sh --locked --testflight-build 11 --testflight-unsigned
```

The archive lands in `dist/testflight/`. See [build 11 packaging](docs/build-11.md) for IPA export and source provenance.

This port is [MIT-licensed](LICENSE). See [third-party notices](THIRD_PARTY_NOTICES.md) for attribution. Horizon iOS is independent of Mojang, Microsoft, Apple, and the upstream Horizon maintainers.
