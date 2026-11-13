# Installing Horizon iOS

Horizon iOS is not distributed through the App Store. Install it by re-signing the unsigned IPA
with your own Apple ID or development team, or by building it in Xcode. Either way, **you supply
your own Minecraft 26.3 data**; the app and its releases contain none.

> Horizon iOS is an unofficial project. It is not approved by or associated with Mojang, Microsoft,
> Apple or the upstream Horizon maintainers. Minecraft is a trademark of Mojang Synergies AB.

## Before you start

- An arm64 iPhone or iPad on a version listed in the downstream validation notes.
- A Mac (for resources and, optionally, the build) with **Minecraft: Java Edition 26.3** installed
  and played once from the official launcher.
- A free or paid Apple ID. Free Apple IDs produce signatures that expire after **7 days**; a paid
  Apple Developer account lasts a year. Sideloading tools differ in what they require; follow the
  tool's own instructions.
- Free space: the prepared folder is roughly 300–500 MB, and the app keeps a verified copy inside
  its container, so allow at least **1.5 GB** free on the device. First import can take several
  minutes on older hardware.

## Step 1: Get an app

### Option A — unsigned IPA (recommended if you have a Mac)

Download `HorizonIOS-<version>-unsigned.ipa` from the release, or build it yourself with
[BUILDING.md](BUILDING.md). The IPA is a standard unsigned app bundle: it contains no Apple
signature, provisioning profile, Minecraft assets or credentials.

Re-sign and install it with a tool that accepts your own Apple ID, for example Sideloadly or
AltStore. The exact steps are owned by those tools; in outline:

1. Connect the device and trust it.
2. Select the IPA and your Apple ID/team.
3. Let the tool create or reuse a development profile.
4. Install and trust the developer certificate under **Settings → General → VPN & Device
   Management** if iOS asks.

The default bundle identifier is `dev.horizon.ios`. Change it in your signing tool if it collides
with another app, or if your team requires a unique identifier.

### Option B — Xcode

Open `ios/HorizonIOS.xcodeproj`, select your team under **Signing & Capabilities**, change the
bundle identifier if needed, choose your device, and Run. Xcode handles provisioning. See
[BUILDING.md](BUILDING.md) for the command-line equivalent.

> Re-signing with a different bundle identifier creates a separate app with separate saves. Keep
> the identifier stable across updates.

## Step 2: Prepare your own resources

On the Mac, follow [BUILDING.md §4–5](BUILDING.md#4-prepare-your-own-minecraft-263-resources). The
result is a folder named `HorizonResources` containing `mobile-import.json`,
`horizon-resources.json` and the prepared data.

You can prepare it on any Mac and transfer the folder to the Mac that manages the device. The
preparation tools do not download Minecraft for you and do not bypass ownership checks.

## Step 3: Put the resources in the app

The app reads `Documents/HorizonResources` inside its own container. There are two practical ways
to fill it.

### Finder file sharing (iOS 16+)

1. Launch Horizon iOS once. It creates its Documents folder and shows the "resources needed"
   message.
2. Connect the device to the Mac and open Finder.
3. Select the device, open the **Files** tab, and choose **Horizon iOS**.
4. Drag the whole `HorizonResources` folder into the app's Documents area. Do not rename it and do
   not place it inside another folder.
5. Force-quit the app and launch it again.

### Files app

If you transferred the folder with AirDrop, iCloud Drive or a USB drive, open the **Files** app,
go to **On My iPhone/iPad → Horizon iOS**, and copy `HorizonResources` there. The folder must sit
directly in the app's Documents directory.

The app verifies every file against the SHA-256 index before opening a world. Copying the folder in
place is not enough for a running app: relaunch it after the copy. If verification fails, the app
reports the error and keeps the previous verified import; replace the folder with a complete copy
and relaunch.

## Step 4: First launch

1. **Loading resources** — the app verifies and copies the import on a worker. The first pass is
   the slowest.
2. **Preparing graphics** — Metal pipelines and textures are created. A cold pipeline cache can
   take a while.
3. **World** — the bundled Creative world opens on the integrated server. Terrain appears as
   chunks stream in.

The default render distance is deliberately small (2 chunks) for the first launch. Raise it in
**Options → Video Settings** on hardware that can afford it; later launches keep your choice.

## Controls

| Action | Control |
| --- | --- |
| Move | Drag the lower-left pad |
| Look | Drag the right side of the screen |
| Jump | Hold **Jump** at the right edge |
| Break / attack | Hold **Break** |
| Place / use | Hold **Place** |
| Pick block | Tap **Pick block** |
| Inventory | Tap **Inventory** (top right) |
| Pause / Back | Tap **Back / Pause** (top left) |
| Chat | Tap **Chat**, then use the iOS keyboard |
| Type in a field | Tap the field; the native keyboard appears |

Back follows the current screen's own cancel action. The keyboard's **Done** commits a single-line
field; in a multi-line field it only hides the keyboard.

## Multiplayer

Choose **Multiplayer → Direct Connection** (or the address field on the multiplayer screen), enter
a host and optional port, and connect. Bracketed IPv6 addresses such as `[2001:db8::1]:25565` are
supported.

- Servers in **offline mode** are the tested path.
- **Online-mode** servers need Microsoft authentication, which the iOS build has not validated.
  Treat it as unsupported in this release.
- Older GrimAC/PacketEvents builds can kick any 26.3 client when several players join; this is a
  server-side plugin problem and was reproduced with vanilla clients. Update the server plugins.

## Saving, updating and removing

- **Save and quit** from the pause menu. The app also asks the server to save and flush when it is
  backgrounded, within iOS's bounded background time. Wait for the save message before assuming the
  world is durable; a force-quit during a save can lose the last changes.
- **Update** by installing the new build over the old one with the same bundle identifier. Saves
  live in the app container and survive an update. Deleting the app deletes its saves and imported
  resources.
- **Remove** the app from the home screen. Nothing outside its container is modified.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| "Resources needed" after copying | The folder must be `Documents/HorizonResources` with `mobile-import.json` directly inside it. Force-quit and relaunch. |
| Verification failed | The copy is incomplete or from a different 26.3 build. Re-run `pack-resources.py` and replace the whole folder. |
| App opens to a cube | Prepared resources were not found. The label repeats the folder path; check Finder file sharing. |
| Install fails with a provisioning error | The profile does not include this device or the identifier is taken. Re-sign with a profile that lists the device, or change the bundle identifier. |
| App expires after a week | A free Apple ID signature expired. Re-sign with your tool. |
| Very low frame rate | Lower render distance and resolution in Video Settings, close other apps, and let the device cool. Generation is the most expensive moment. |
| Multiplayer disconnects immediately | Check the server's online-mode setting and plugin versions; read the server log for a plugin-issued kick. |
| Keyboard covers the field | The editing strip sits above the keyboard by design; rotate or use Done/Back. |

If the app crashes, the device's **Settings → Privacy & Security → Analytics & Improvements →
Analytics Data** may contain a Horizon crash report. Include it and the Xcode/device console log in
a bug report. See [KNOWN_ISSUES.md](KNOWN_ISSUES.md) and [CONTRIBUTING.md](CONTRIBUTING.md).
