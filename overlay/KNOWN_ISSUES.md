# Known issues — `v0.2.0-beta.11` (preview)

This file records what is known to be missing or unreliable. Absence of an item here is not proof
that it works; see [docs/ios-validation.md](docs/ios-validation.md) for what was actually tested.

## Release blockers for a "stable" label

There are no hidden critical issues, but the following are why this release remains a **preview**
and why no App Store or universal-install claim is made.

### Important

1. **No App Store or universal IPA.** The project has no notarized, signed-for-distribution build
   and no Apple distribution identity. An unsigned IPA must be re-signed by the user. The app is
   not "ready to install" without that step.
2. **Resources are user-supplied.** The IPA contains no Minecraft data. Players must prepare 26.3
   data on a Mac and import `HorizonResources` with the in-app Files picker. There is no in-app
   download or account-linked fetch.
3. **Online-mode multiplayer is unvalidated.** Offline-mode servers and the integrated server are
   the tested paths. Microsoft account login from iOS has not been verified. Treat online-mode
   servers as unsupported.
4. **Performance is not characterised.** The figures in the README are user observations from
   specific devices, not controlled benchmarks. No thermal, long-session, memory-pressure or
   frame-time study exists for this build. Older 2 GB iPads can be evicted under memory pressure.
5. **Older GrimAC/PacketEvents servers kick 26.3 clients.** The failure is server-side and also
   reproduces with vanilla 26.3 clients. Updating Grim/PacketEvents (Grim 2.3.75+ in the original
   reproduction) removes it. Horizon needs no compatibility workaround. See
   [docs/ios-server-compatibility.md](docs/ios-server-compatibility.md).
6. **Modified-block persistence on physical devices needs a fresh check.** World save/relaunch and
   player-position persistence were verified, but a device-side break/place → save → relaunch
   round-trip is not fully recorded for the current build.
7. **iPad 5 (A9) validation predates this exact tree.** The older build was played on iPadOS
   16.7.16, but the current Release build was not re-validated on that iPad. Do not present the
   iPad figures as a current-build benchmark.

### Optional

- Controller support is unavailable, and audio does not continue in the background.
- No mod browser, Realms, skins marketplace, telemetry opt-in UI or self-updater.
- The app name, icon and first-run screen are functional, not final branding. There is no custom
  app icon yet.
- The default first-launch render distance is 2 chunks. Raise it in Video Settings; settings persist
  after the first launch.
- No iPad multitasking: the app is landscape-only and full-screen.

## Resource import

- Importing copies and hashes tens of thousands of files. A cold import can take minutes on an A9;
  a warm launch re-verifies the existing copy and is faster.
- The import source is `Documents/HorizonResources`, or a developer-staged
  `PreparedResources` folder inside the app bundle. If both exist, the bundle wins.
- Adding files to Documents while the app is running does not hot-reload them. Relaunch.
- A failed verification keeps the last good import and shows an error; it does not fall back to a
  partially copied tree.
- `pack-resources.py` excludes Mac-only caches and exporter bytecode. Do not re-add them.

## Saves and lifecycle

- Backgrounding requests an asynchronous save and flush within iOS's background budget. If iOS
  expires the task, completion is logged as unconfirmed.
- A force-quit or crash during a save can lose recent changes. There is no crash-safe journal beyond
  the upstream save format.
- Only the bundled `Mobile Creative` world is created automatically. Creating additional worlds,
  survival/hardcore selection and world settings are available only through the upstream screens
  that the current host exposes; they have not been validated on iOS.
- Deleting the app deletes its saves and imported resources.

## Input and UI

- Touch controls are a Bedrock-inspired overlay over Java controls, not full native Bedrock
  parity. Layout customization exists, but there is no controller glyph set, Bedrock touch-target
  ring, or full vehicle-control parity.
- The keyboard first-responder is a single shared `UITextView`; composition, paste and selection
  are tested, but third-party keyboards and hardware keyboards have not been exhaustively tested.
- The on-screen debug labels and log lines are developer-facing and not localized beyond English
  strings in `ios/Resources/en.lproj`.
- Screens that assume a mouse wheel or a precise cursor may be awkward on a touch screen.

## Networking

- Login, configuration and play use the upstream protocol 777 codecs. Online-mode encryption and
  Microsoft session authentication are not exercised on iOS.
- Simultaneous physical clients were observed in earlier validation, but the current build's
  multi-device acceptance matrix is incomplete.
- Portal/dimension transitions, reconnects and server transfers have unit/mock coverage but not a
  full physical journey in this release.

## Platform and build

- No jailbreak, JIT, interpreter, dynamic code loading or private API is required or used.
- The app is arm64 only. See the downstream validation notes for the deployment floor. There is no Intel-simulator or x86_64 slice.
- Newer Xcode versions may not offer device support for iOS 16; installing on an iOS 16 device can
  require older Apple tooling or a sideloading tool. This is outside the app's control.
- CI builds and packages the app but does not run gameplay: there are no Minecraft assets in CI and
  no iOS hardware. Runtime claims rest on the validation record, not CI.
- The Xcode project is generated by XcodeGen. Editing the `.xcodeproj` by hand is overwritten the
  next time `project.yml` is regenerated.

## Reporting

Include the device model, iOS version, build identifier/commit, whether resources were freshly
prepared, the exact steps, and the console log if available. Do not attach Mojang game files,
world saves containing personal data, tokens or provisioning profiles. See
[CONTRIBUTING.md](CONTRIBUTING.md).
