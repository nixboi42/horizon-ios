# Horizon iOS

An unofficial native iPhone and iPad port of [Horizon](https://github.com/horizon-rust/horizon),
the Rust rewrite of Minecraft: Java Edition 26.3. This repository contains the downstream builder,
patch series, and iOS overlay. It is not affiliated with Mojang, Microsoft, Apple, or the Horizon
maintainers.

## Install

This is an unsigned, sideloaded app, not an App Store release. Re-sign the public IPA with your own
Apple ID or development team using a sideloading tool, then install it. Keep the bundle identifier
stable when updating so the app retains its saves. See [INSTALL.md](overlay/INSTALL.md) for the
device setup and resource import walkthrough.

## Prepare your own game resources

The IPA intentionally contains no Minecraft assets, jars, or prepared resources. On a Mac, install
and launch the official Minecraft Java Edition 26.3, then prepare the matching files with the
official Horizon preparation tools. The builder can prepare resources from your owned installation;
see [BUILDING.md](overlay/BUILDING.md). Copy the resulting `HorizonResources` folder to
`Documents/HorizonResources` in the app's Files container and relaunch.

## Build

Requires an Apple silicon Mac, full Xcode, Git, Python 3.11+, and Rust. Install the iOS target with
`rustup target add aarch64-apple-ios`. To reproduce the locked public IPA:

```sh
./build.sh --locked --public
```

The asset-free IPA, build manifest, and SHA-256 file are written under `dist/`. For resource
preparation, Xcode options, and other builder modes, see [BUILDING.md](overlay/BUILDING.md).

## Features

- Integrated single-player world, touch movement and actions, inventory, save and relaunch.
- Direct-address multiplayer using Horizon's 26.3 protocol; offline-mode servers are the validated path.
- Native iOS keyboard editing and sandboxed, hash-verified resource import.
- Arm64 iPhone and iPad support, with an iOS 15.0 deployment target.

## Known limitations

- No launcher, App Store distribution, Microsoft sign-in, audio, or controller support.
- Online-mode multiplayer and sustained performance are not validated for this release.
- Resources must be prepared on a Mac and imported manually; allow about 1.5 GB of free device space.
- Landscape-only, full-screen UI; older devices may be evicted under memory pressure.
- The iOS 15.0 value is the compiled deployment target, not a physical test result. Prior port
  validation ran on iPhone 13 mini with iOS 27.2.1 and iPad 5 with iPadOS 16.7.16. The locked
  release revision was not physically retested. See [validation notes](docs/validation.md).

See [known issues](overlay/KNOWN_ISSUES.md) and [release notes](RELEASE_NOTES.md) for details.

## License and attribution

The downstream port is MIT-licensed; see [LICENSE](LICENSE). Upstream Horizon source remains under
its own license. Dependency notices and attribution are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
and [upstream.lock](upstream.lock).
