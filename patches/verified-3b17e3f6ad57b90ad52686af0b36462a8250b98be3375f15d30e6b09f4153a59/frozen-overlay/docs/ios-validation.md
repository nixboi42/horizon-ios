# iOS validation record

This is a summary of what was actually run, on which hardware, and what remains unverified. It is
not a benchmark report and not a guarantee for every device.

## Environment

| Item | Value |
| --- | --- |
| Xcode | 27.2 (27B52), iPhoneOS SDK 27.2 |
| Rust | 1.99.1; workspace minimum 1.95 |
| Simulator | iPhone 17 Pro, iOS 27.0, Apple M5 host |
| Phone | iPhone 13 mini, A15, iOS 27.2 |
| Tablet | iPad 5 (`iPad6,11`), A9, 2 GB RAM, iPadOS 16.7.16 |
| Base upstream | Horizon `18840db9` (`upstream/main`, 2026-11-07) |

The current tree's iOS changes were developed on top of that upstream revision. Gameplay, world
format and protocol behaviour are upstream's; the port adds the host and the platform branches
listed in [ios-architecture.md](ios-architecture.md).

## What was verified

The physical-device and simulator interaction results below were recorded before the final
release-only changes (Documents resource import, first-run settings persistence and packaging
tooling). The exact `v0.1.0-experimental` unsigned IPA was built and statically verified on the
host, but not installed on a device; see [Release artifact](#release-artifact-2026-11-08).

### Rust and host checks

| Check | Result |
| --- | --- |
| `cargo test -p horizon-ios --lib` | Passed (14 tests; the private-resource streaming test is ignored) |
| Resource-import unit tests | Passed: atomic repeatable import, corruption detection, unsafe paths, symlinks, wrong version, warm verification, bundle/Documents selection |
| Flight and touch-ownership tests | Passed: simultaneous move/look/actions, cancellation, balanced releases, camera chunk boundaries |
| `cargo check` for `aarch64-apple-ios` on the client and host | Passed |
| Clippy with warnings denied, host and iOS target | Passed for the iOS host crate |
| Workspace formatting | Passed |
| Public-tree and release privacy checks | Passed |

### Simulator

| Check | Result |
| --- | --- |
| arm64 simulator Release build | Passed |
| App launch with prepared resources | Passed: import, resource loading, Metal pipeline creation, world handoff |
| Simulated gameplay | Passed: movement, look, jump, break/place with server confirmation, inventory, pause/resume |
| Save and relaunch | Passed on the simulator, including a remeshed chunk corner |
| Native keyboard UI tests | Passed: focus/text across background/resume, chat deletion/dismissal/reopening, repeated pause/resume and inventory Back |
| Multiplayer session fixture | Passed with a mock peer using production login/configuration/play codecs and decoded movement/attack/use packets |

The simulator uses an Apple-silicon host and is not a performance proxy for a phone or a 2 GB iPad.

### Physical iPhone 13 mini

| Check | Result |
| --- | --- |
| Signed arm64 Release build | Passed; installed and launched with development signing |
| Integrated server and world startup | Passed; real chunks rendered, roughly 20 active TPS in the recorded session |
| Touch controls | User-confirmed walking/collision, held Jump, break and place through direct touch |
| Native keyboard | Automated UI cases passed: background/resume focus, chat deletion/dismissal/reopening, pause/inventory Back |
| Background save | Bounded asynchronous save/flush completed; a fresh process restored the saved player position |
| Modified-block persistence | Not re-verified end-to-end on this build |

The user-confirmed interaction is separate from the automated tests. Device Hub drag injection did
not work in the recorded session and was not counted as evidence.

### Physical iPad 5

An earlier build started and rendered on iPadOS 16.7.16, and the owner observed roughly 60 FPS in
normal play and 40–50 FPS during heavy generation at 50% render resolution and a 10-chunk setting.
Those are user observations, not controlled measurements.

The current Release build was **not** re-validated on the iPad. A later installation attempt on the
iPad failed before launch because the available provisioning profile did not include that device;
this is a signing limitation, not an app crash. Do not treat the older figures as a current-build
benchmark, and do not claim iPad support beyond "the same arm64/iOS 16 build is expected to work".

### Release artifact (2026-11-08)

The release candidate was rebuilt from the final tree and checked as a packaged artifact, without
using Minecraft data or signing credentials:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --locked -p horizon-ios --lib` | Passed: 14 tests, 1 ignored |
| Clippy (host, all targets) | Passed with `-D warnings` |
| Clippy (`aarch64-apple-ios`, library) | Passed with `-D warnings` |
| Python release-privacy tests | Passed: 7 tests |
| `tools/check_public_tree.py` | Passed |
| `ios/scripts/check_release.py` | Passed |
| Simulator Release build | Passed |
| Device Release build | Passed |
| `ios/scripts/verify-app.sh` on the extracted IPA payload | Passed |
| `ios/scripts/check_release.py --archive` / `--tree` | Passed, including the executable's byte scan |
| Codesign state of the payload | Unsigned (`code object is not signed at all`) |
| Binary architecture / minimum OS | arm64; `minos 16.0`, SDK 27.2 |

The IPA contains seven entries: the bundle directory, `HorizonIOS`, `Info.plist`, `PkgInfo`,
`en.lproj/Localizable.strings` and their parent directories. It has no `PreparedResources`, JARs,
worlds, provisioning profile, signature or credentials. SHA-256:
`1b8fa2bb5c0a43030a3fe53ff83395aa6bd7a13dec6c26d8e1eccf6a4c86b1c8` (29,299,505 bytes).

The device build was not installed on hardware for this pass, so keyboard, touch, world and save
behaviour remain covered only by the earlier runs above.

## What remains unverified

- Online-mode Microsoft authentication from iOS.
- A complete physical multi-device multiplayer journey, including reconnects and dimension changes.
- Long sessions, thermal throttling, memory pressure and eviction on 2 GB devices.
- Modified-block persistence after a save/relaunch on a physical device.
- Audio and controller support (both are compiled out).
- iPad 5 performance for the current build.
- App Store, notarized or distribution-signed packaging.

## Reproducing the checks

```sh
cargo test --locked -p horizon-ios --lib
cargo clippy --locked -p horizon-ios --all-targets -- -D warnings
cargo clippy --locked -p horizon-ios --lib --target aarch64-apple-ios -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests -p 'test_ios_*.py'
python3 tools/check_public_tree.py
python3 ios/scripts/check_release.py

./ios/scripts/build-app.sh --platform simulator --configuration Release
./ios/scripts/verify-app.sh .local/xcode-simulator/Build/Products/Release-iphonesimulator/HorizonIOS.app --private
./ios/scripts/build-app.sh --platform device --configuration Release
./ios/scripts/make-ipa.sh --app .local/xcode-device/Build/Products/Release-iphoneos/HorizonIOS.app
python3 ios/scripts/check_release.py --archive dist/HorizonIOS-0.1.0-unsigned.ipa
```

`--private` is only for a local app bundle that embeds prepared resources. `make-ipa.sh` strips
that folder, signatures and provisioning profiles from the published archive by default; pass
`--allow-resources` only for a private local build that must never be shared.

The ignored streaming test needs the owner's private prepared resources:

```sh
HORIZON_IOS_TEST_RESOURCES="$PWD/.local/horizon-client/26.3" \
  cargo test --locked -p horizon-ios --lib terrain::tests::real_world -- --ignored --nocapture
```

Simulator UI tests need prepared resources in the app bundle and a booted simulator; see
[BUILDING.md](../BUILDING.md). The gameplay and parity harnesses under `tools/` need the upstream
reference data and are not run in CI.

## Reporting a device result

Include the device model, iOS version, build commit/version, whether resources were freshly prepared,
and the console log. Redact account names, server addresses, device identifiers and tokens. See
[CONTRIBUTING.md](../CONTRIBUTING.md).
