# iOS architecture

Horizon iOS is a platform host around the existing Horizon client. It does not replace the
renderer, gameplay, server or save format. Everything below is the platform boundary and the
places where the shared code needed a portable fix.

## Layers

| Layer | Where | Responsibility |
| --- | --- | --- |
| UIKit and SwiftUI shell | `ios/App/main.m`, `ios/App/Launcher.swift`, `ios/App/Info.plist` | Scene adoption, the native launcher/settings form, safe-area Back/Chat controls, text responder, background-save task |
| Rust host | `crates/horizon-ios/src/lib.rs` | One winit event loop/window, sandbox paths, resource import, launcher commands, gameplay session lifecycle, Metal surface |
| Platform fixes | `crates/horizon-client`, `horizon-client-input`, `horizon-client-render`, `horizon-threads`, `horizon-protocol`, `horizon-hdr` | Small `cfg(target_os = "ios")` branches and portable fixes |
| Gameplay | upstream `horizon-client` | Resources, world loading, integrated server, remote client, rendering, menus, saves |
| Packaging | `ios/scripts/`, `.github/workflows/ios.yml` | Build, stage, verify, package, privacy gate |

`horizon-ios` links `horizon-client` with default features off (no audio, no controllers). It is a
static library plus a small fixture binary; the app target links it as `libhorizon_ios.a`.

## Process and lifecycle

`main.m` resolves the app's Application Support, Caches, bundle and Documents paths, then calls
`horizon_ios_run`. winit owns the UIKit process and creates its one window. `Launcher.swift` attaches
a SwiftUI `UIHostingController` over that window for the launcher and Settings form; it does not
create another window or render loop. The Rust side:

1. Creates the sandbox layout before any worker starts and changes the process working directory
   to `Application Support/HorizonIOSDiagnostic/Gameplay` (or a validated `Tests/<session>` folder
   for disposable UI tests).
2. Points `HORIZON_HOME`, launcher homes, XDG folders and cache folders into that sandbox, and
   disables self-update and audio.
3. Starts the resource importer on a named worker, then enters `UIApplicationMain` through winit.
4. Presents the SwiftUI launcher immediately; the importer never blocks the first UIKit frame.

Play and Return are SwiftUI actions that store a command and send a winit user event. The command
is handled on the existing event-loop thread. Play constructs the ordinary `game::mobile::Session`
using the imported resource tree and the saved offline username. Return releases touch inputs,
starts the game's staged world-close save, and polls it between winit waits. Once the integrated
server has saved and stopped, the host drops the session on the event-loop thread, which releases
the network client, workers and renderer. The window, event loop, resource import and saved worlds
remain available for the next Play. No second `UIApplicationMain`, `EventLoop`, renderer or gameplay
implementation is created.

The app declares one `UIWindowScene` because current iOS requires scene adoption. The scene
delegate attaches winit's real `UIWindow` through its public `UIView` handle. There is no second
render loop, `MTKView`, WebView or streaming client.

Suspension releases input, stops frame requests and drops the Metal surface; resume recreates it.
When gameplay is active, suspension pauses the integrated server and requests a bounded background
save/flush. UIKit grants the background task; the host polls for an explicit completion receipt and
logs when iOS expires the time.

## Rendering

`wgpu` selects Metal only, with FIFO presentation and a 60 FPS pacing target. The host uses
`outer_size` for the drawable because winit 0.30 reports the safe-area rectangle as `inner_size`.
Depth and surface state are recreated on resize, and surface lost/outdated/suboptimal are handled
separately from fatal device loss. A device-loss recovery path for every GPU object is still future
work; a fatal loss can terminate the app.

The real client renderer uses the ordinary Horizon passes. The optional diagnostic scenes are the
only exception:

- `HORIZON_IOS_SCENE=synthetic` draws a baked cube.
- `HORIZON_IOS_SCENE=terrain` runs the historical streaming flight viewer.
- The default (unset) path presents the native launcher and waits for Play.

Diagnostics are development tools, not the release experience.

## Input

Touch input is translated into Horizon's existing actions and key mappings. `gameplay_input.rs`
owns per-touch state: a left movement pad, right-side look, and Jump/Break/Place/Pick buttons. Every
press has an owner, releases always drain, and cancellation/focus loss releases held actions. A GUI
touch becomes a single owned pointer so a second finger cannot release it.

SwiftUI draws the launcher and a small Return to Launcher button while gameplay is active. Its game
overlay only accepts touches in that button's area, leaving the existing touch controls reachable.
UIKit draws the Back, Chat and native text overlay. Back invokes the current screen's own cancel
action (`Game::pad_back` and the special sign/book/block-editor paths). The native text view mirrors
the focused Horizon field and applies committed text and selection through the original widget's
filters, including limits and validation. Selection offsets are converted from UTF-16 to Unicode
scalars. Field identity and focus generation reject stale edits.

## Resources

`pack-resources.py` (Mac) stages the prepared 26.3 tree and writes `mobile-import.json` with a
SHA-256 for every file. The app imports from, in order:

1. `HorizonIOS.app/PreparedResources` (developer build), or
2. `Documents/HorizonResources` (player-provided, visible through Finder file sharing).

`resources.rs` rejects unsafe paths, symlinks, non-regular files, wrong schema/data version and any
hash mismatch. It copies into a staging directory, re-verifies the copy, then publishes by rename.
The destination is content-addressed by the index hash, so a new import does not overwrite an
existing one and a warm launch verifies the destination instead of recopying the source.

The import proves transfer integrity, not ownership or authenticity. The player must own the game
and prepare the data themselves.

## Networking and saves

Multiplayer uses the upstream protocol 777 codecs and the same login/configuration/play state
machine as desktop. Login and configuration run on the network thread; the game thread polls
events. The integrated server runs on its own thread with `NetConfig.listen=false` and an in-memory
pipe to the client.

Saves live in the app container under `Gameplay/saves`. The host never opens a developer world or a
path outside its sandbox. Test sessions are separate validated folders. The save format is the
upstream format; there is no iOS-specific world conversion.

## Platform fixes in shared crates

These are deliberately small and portable:

- `horizon-threads`: iOS uses public Mach `task_info` for resident memory instead of macOS
  `libproc`; the clipboard uses `UIPasteboard`.
- `horizon-client-input`: iOS uses UIKit pasteboard and logical key labels instead of desktop
  modifier supplements.
- `horizon-client-render`: Metal backend selection, no Vulkan/GLES probing, iOS-safe flat-draw
  capability checks, and surface suspend/resume.
- `horizon-protocol`: server-issued disconnect diagnostics distinguish a kick from a local codec
  error.
- `horizon-hdr`: Vulkan-only paths are excluded.

Do not fork these systems. If a change is needed, keep the desktop behaviour and add the smallest
platform branch.

## Tests and diagnostics

- Unit tests live with each module; `cargo test -p horizon-ios --lib` covers import, flight, touch
  ownership and camera bounds.
- The ignored `terrain::tests::real_world` test needs private prepared resources and exercises the
  real generator/light/mesher pipeline.
- `HORIZON_IOS_TEST_SESSION` selects a disposable game folder for UI tests.
- `HORIZON_IOS_CONTROL=1` exposes the upstream control socket at `ctl.sock` inside that folder for
  local agent tests. It opens no network port.
- The gameplay, LAN and parity harnesses under `tools/` remain the source of truth for shared
  behaviour.

Runtime results and gaps are in [ios-validation.md](ios-validation.md). Build commands are in
[../BUILDING.md](../BUILDING.md).
