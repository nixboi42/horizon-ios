# Horizon iOS v0.1.0-alpha.1

First public alpha of the native iPhone and iPad port of Horizon for Minecraft: Java Edition 26.3.
This is a source and unsigned IPA release for experienced sideloaders. It is unofficial and is not
approved by or affiliated with Mojang, Microsoft, Apple, or the upstream Horizon project. Minecraft
is a trademark of Mojang Synergies AB.

## Downloads

- `HorizonIOS.ipa` — unsigned, arm64, asset-free public IPA (29,277,533 bytes).
- `SHA256SUMS` — checksum for `HorizonIOS.ipa`.
- Source — this standalone downstream repository at tag `v0.1.0-alpha.1`.

SHA-256:

```text
c2266409ac9b2f17e4593f81b6ed9ca84cf0556d8b04eb0666f02424fb67e4dc  HorizonIOS.ipa
```

The IPA compiles and packages for iOS 15.0 and supports iPhone and iPad. No iOS 15 device or
simulator was physically verified. Previous port validation ran on an iPhone 13 mini
with iOS 27.2.1 and an iPad 5 with iPadOS 16.7.16. The locked e9358e2 release revision was not
physically retested. See [validation notes](docs/validation.md).

The public IPA contains no Minecraft jars, assets, prepared resources, saves, credentials,
provisioning profiles, or Apple signing identity. It must be re-signed by the user before
installation. See [installation](overlay/INSTALL.md).

## Highlights

- Integrated single-player gameplay with touch controls, world saving, and relaunch.
- Direct-address multiplayer; offline-mode servers are the validated path.
- Native keyboard editing and verified manual resource import from the app's Documents folder.
- Reproducible downstream source builder pinned to upstream commit
  `e9358e2892e1a348aca5e96fa9308458d7566e5d` and the eight-entry patch series.

## Limitations

- No App Store distribution, launcher, Microsoft sign-in, audio, or controller support.
- Online-mode multiplayer, long-session performance, and current-revision physical-device behavior
  are unverified.
- Resources must be prepared from the user's owned Minecraft 26.3 installation on a Mac and copied
  manually to the app. The app is landscape-only and full-screen.
- Older GrimAC/PacketEvents versions can reject 26.3 clients; update the server plugins.

## Verify

```sh
shasum -a 256 HorizonIOS.ipa
unzip -l HorizonIOS.ipa
```

Build the public artifact from source with `./build.sh --locked --public`. See
[BUILDING.md](overlay/BUILDING.md) and [known issues](overlay/KNOWN_ISSUES.md).
