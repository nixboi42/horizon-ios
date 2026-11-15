# Building Horizon iOS

This guide builds the app and packages a re-signable IPA from source. It never downloads,
prepares or redistributes Minecraft data automatically: you point the tools at your own
installation.

## 1. Requirements

| Component | Tested | Notes |
| --- | --- | --- |
| macOS | 15 or later on Apple silicon | Intel Macs are not supported for the iOS simulator slice used here |
| Xcode | 27.2, iPhoneOS SDK 27.2 | Full Xcode, not only Command Line Tools; Xcode 16+ should work but is untested |
| Rust | 1.99.1 | The workspace minimum is 1.95; install with rustup |
| Python | 3.10+ | The macOS system Python 3.9 is too old for some upstream tools |
| Minecraft | Java Edition 26.3 | Owned and installed once through the official launcher |
| Java | the JDK pinned by `research-lock.json` (25 at this revision) | Needed only for Mac-side resource preparation |
| XcodeGen | 2.x | Optional; only to regenerate the checked-in Xcode project |

Xcode's device support starts at iOS 17 on recent versions. Installing on an iOS 16 device may
need an older Xcode, a matching developer disk image, or a sideloading tool. This is an Apple
tooling limitation, not an app requirement; the default deployment target is defined in `ios/Deployment.xcconfig`.

## 2. Install the Rust targets

```sh
rustup toolchain install stable
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
rustup component add rustfmt clippy
```

The Xcode build phase uses a workspace-local installation when one exists at
`.local/cargo/bin/rustup` with `RUSTUP_HOME=.local/rustup` (set automatically). Otherwise it uses
`cargo` from `PATH`. It never edits or replaces your global Rust installation.

Only arm64 device and arm64 simulator builds are produced. An Intel Mac cannot run the arm64
simulator, and the project does not build an x86_64 slice.

## 3. Select Xcode

```sh
xcode-select -p                 # must point into Xcode.app
xcodebuild -version
xcrun --sdk iphoneos --show-sdk-version
xcrun simctl list devices
```

If the path is wrong, select Xcode in **Xcode → Settings → Locations** or run
`sudo xcode-select -s /Applications/Xcode.app`.

## 4. Prepare your own Minecraft 26.3 resources

This is the same preparation the desktop game uses. It runs Java exporters on your Mac and writes
to `.local/horizon-client/26.3`, which Git ignores.

```sh
cargo build --release -p horizon-prepare
./target/release/horizon-prepare --prepare --server-jar /absolute/path/to/your/26.3/server.jar --complete
./target/release/horizon-prepare --check
```

`--prepare` imports the pinned game files from your own copy. `--complete` also copies sounds and
then requires `--check` to pass. If your install is somewhere unusual, add `--minecraft-dir`,
`--version-json`, `--client-jar`, `--libraries-dir` or `--assets-dir`; see
`./target/release/horizon-prepare` with no arguments for the full list.

The audio-enabled iOS candidate needs sounds for playback. A smaller silent import remains possible with
`horizon-prepare --prepare --server-jar ...` (without `--complete`), and
`pack-resources.py --without-sounds` omits audio from the staged copy that the app imports.

## 5. Stage resources for the app

Build a `HorizonResources` folder. The packer verifies schema 7 / data version 5023, runs
`horizon-prepare --check` against the same output, rejects symlinks, executable payloads, caches and
Mac-only export bytecode, and writes a SHA-256 index:

```sh
python3 ios/scripts/pack-resources.py .local/horizon-client/26.3 \
  --dest "$PWD/HorizonResources"
```

Use `--preparer` if `target/release/horizon-prepare` is elsewhere. `--without-sounds` produces a
silent copy for a smaller transfer. `--terrain-chunk` is only for the historical terrain diagnostic
and is not needed for gameplay.

Never commit `HorizonResources` or `ios/Resources/PreparedResources`; both are ignored by Git, and
the release privacy check rejects them from a published bundle or archive.

## 6. Build the app

### Command line

```sh
./ios/scripts/build-app.sh --platform device --configuration Release
./ios/scripts/build-app.sh --platform simulator --configuration Release
```

The first command writes an **unsigned** `.app` under `.local/xcode-device/`; no Apple account is
used. The second writes a simulator app under `.local/xcode-simulator/` (or set
`HORIZON_DERIVED_DATA`).

For a signed development build with your team:

```sh
DEVELOPMENT_TEAM=ABCDE12345 ./ios/scripts/build-app.sh --platform device --signed
```

`--signed` enables automatic signing using existing local provisioning. Keep team IDs, certificates
and profiles out of Git; the privacy check rejects common mistakes.

The Xcode project is checked in. Regenerate it only after editing `ios/project.yml`:

```sh
xcodegen generate --spec ios/project.yml
```

### Xcode

Open `ios/HorizonIOS.xcodeproj`, select the `HorizonIOS` scheme, choose your team under
**Signing & Capabilities**, change the bundle identifier if your team requires it, and Run. The
scheme's pre-build phase compiles `libhorizon_ios.a` for the selected platform and configuration.

## 7. Package a re-signable IPA

```sh
./ios/scripts/make-ipa.sh
```

It builds the unsigned device app if needed, strips embedded prepared resources, signatures and
provisioning profiles from the archive, runs the release privacy check, and writes
`dist/HorizonIOS-<version>-unsigned.ipa` with its SHA-256. Use `--app PATH` to package an existing
build and `--output PATH` to choose the destination. `--allow-resources` keeps prepared resources
and is only for a private local build that must never be shared.

Install it through Sideloadly, AltStore or another re-signing tool, or sign it with your own
development team. See [INSTALL.md](INSTALL.md).

## 8. Verify the build

```sh
APP=.local/xcode-device/Build/Products/Release-iphoneos/HorizonIOS.app
./ios/scripts/verify-app.sh "$APP"
./ios/scripts/check_release.py --tree "$APP"
```

`verify-app.sh` checks the identifier, version, matching Info.plist and Mach-O deployment targets, arm64-only slice and the absence
of embedded profiles or prepared resources. Pass `--private` when verifying a local development app
that intentionally embeds your prepared resources. The privacy check also rejects game data,
credentials, absolute home paths and signing material. To verify what will actually be published,
extract the IPA and check that payload instead:

```sh
WORK=$(mktemp -d)
unzip -q dist/HorizonIOS-0.1.0-unsigned.ipa -d "$WORK"
./ios/scripts/verify-app.sh "$WORK/Payload/HorizonIOS.app"
python3 ios/scripts/check_release.py --tree "$WORK/Payload/HorizonIOS.app"
```

## 9. Tests and CI

Fast host checks:

```sh
cargo test --locked -p horizon-ios --lib
cargo clippy --locked -p horizon-ios --all-targets -- -D warnings
cargo clippy --locked -p horizon-ios --lib --target aarch64-apple-ios -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests -p 'test_ios_*.py'
python3 ios/scripts/check_release.py
```

The compiled iOS library checks:

```sh
cargo check --locked -p horizon-ios --lib --target aarch64-apple-ios
cargo check --locked -p horizon-client --no-default-features --lib --target aarch64-apple-ios
python3 tools/check_public_tree.py
```

The simulator UI tests need real resources and a running simulator:

```sh
xcodebuild -project ios/HorizonIOS.xcodeproj -scheme HorizonIOS \
  -configuration Release -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,id=YOUR_SIMULATOR_UDID' \
  -derivedDataPath .local/xcode-ui-tests CODE_SIGNING_ALLOWED=NO test
```

The gameplay, LAN and parity harnesses under `tools/` need the owner's reference files and are not
run in CI. See [docs/parity-workflow.md](docs/parity-workflow.md) and
[docs/ios-architecture.md](docs/ios-architecture.md).

## 10. Reproducibility notes

- `Cargo.lock` pins every Rust dependency. Do not update it as part of a release unless intended.
- Xcode and the build scripts read the default from `ios/Deployment.xcconfig`. Override it with `IPHONEOS_DEPLOYMENT_TARGET`.
- The simulator build uses the host's arm64 slice. CI and the documented commands are verified on
  Apple silicon.
- Build products, Xcode derived data, local toolchains and staged resources live in ignored folders
  (`.local/`, `target/`, `build/`, `dist/`).
- No script contains a personal home path, Apple team ID, device identifier or password. If you add
  one, `ios/scripts/check_release.py` will fail the build.
- The Xcode pre-build script remaps the workspace, Cargo registry and home paths before compiling.
  The release check also scans the compiled executable, so a panic location cannot reintroduce the
  build machine's user name into a published app.
