# iOS port development log

> Historical engineering record, kept for provenance. It describes the port as it was built, so
> some earlier limitations and measurements no longer match the release. Current behaviour is in
> [ios-architecture.md](ios-architecture.md) and [ios-validation.md](ios-validation.md); build
> commands are in [BUILDING.md](../BUILDING.md).

The default app routes its existing UIKit window into Horizon's full
resource loader, real client Game and integrated-server startup. Temporary
touch controls feed upstream gameplay actions; sandbox worlds use the actual
save path. See [Phase 4](#phase-4-native-gameplay-entry-2026-11-07) and
[ios-validation.md](ios-validation.md) for compiled versus runtime-verified
status. The earlier terrain viewer remains an explicit debug scene; its
measurements do not establish gameplay performance. Earlier sections below
are historical evidence. No Java, JIT or runtime module loading is included.

## Baseline and environment

- Official upstream: https://github.com/horizon-rust/horizon
- Base: `18840db9146b2521dd46ad52a4559a49be180f8f`, `upstream/main`, fetched 2026-11-07.
- Local branch: `codex/ios-native-port`. No commits, pushes, releases or PRs made.
- Started from an empty repository. Read `AGENTS.md`, `CONTRIBUTING.md`,
  `ROADMAP.md`, `docs/architecture.md`, `docs/native-client.md` and the affected manifests.
- Xcode 27.2 (27B52), iPhoneOS SDK 27.2, Rust 1.99.1. The original Homebrew Rust
  had only the host target; a separate `.local/` rustup installation supplies
  `aarch64-apple-ios` and `aarch64-apple-ios-sim`. Global Rust was not replaced.
- Paired physical iPhone 13 mini, A15 GPU, iOS 27.2 (24B119), Developer Mode enabled.
  Development signing was already configured. No security settings were changed.
- Simulator: iPhone 17 Pro, iOS 27.0; host confirmed as Apple M5, 24 GiB RAM.
- JDK 25.0.3 is available on the Mac. System Python is 3.9.6; Homebrew Python
  3.15.2 was used for the upstream review check.
- No prepared Minecraft resource folder is present in this checkout. A read-only
  inventory of Prism jar names found 1.12.2 and 26.1.2, not the pinned 26.3.
  No player worlds, account credentials or game settings were read or changed.

The [upstream roadmap](https://github.com/horizon-rust/horizon/blob/18840db9146b2521dd46ad52a4559a49be180f8f/ROADMAP.md)
lists phones as a long-term target. Read-only GitHub inspection found no open PR
matching `ios` on 2026-11-07. [PR #1813](https://github.com/horizon-rust/horizon/pull/1813)
is merged: DualSense compiles on iOS by disabling its unsupported HID/haptics
backend; this is not working controller input. [PR #3730](https://github.com/horizon-rust/horizon/pull/3730)
is closed, not merged; its author says it was intended for their own repository.
Its metadata/discussion were inspected; no implementation was copied from it.
The repository's public claim workflow was not used: this assignment authorizes
local work and expressly withholds authorization to publish a PR. No public
issue or comment was posted.

## Source audit and minimal graph

| Component | Evidence and iOS implication |
| --- | --- |
| `horizon-core` | `src/lib.rs`, `version.rs`, `prepared_files.rs`: pure simulation primitives, version constants and prepared-file reads. Portable. The packed data cache embeds its canonical root path, so a Mac-created cache cannot be reused on iOS. |
| `horizon-assets` | `src/model.rs`, `resolve.rs`, `bake.rs`: actual decoder/resolver/baker used by the client. Portable; directly linked and executed by the diagnostic. |
| `horizon-world` | `Cargo.toml`, `src/nbt.rs`, `region.rs`, `worldgen/`, `level/`: no window dependency. Cross-checks for iOS. World generation still needs prepared tables. |
| `horizon-threads` | `src/process_memory.rs` incorrectly selected macOS libproc on every Apple target. Fixed with public Mach `task_info(MACH_TASK_BASIC_INFO)` on iOS. `worker_count`/`default_workers` already offer worker limits and Apple performance-core detection; no unmeasured scheduling change was added. |
| `horizon-client-input` | `Cargo.toml`, `clipboard.rs`: arboard enabled Wayland code on iOS and failed inside wayland-backend. Replaced only the iOS backend with actual UIPasteboard reads/writes through horizon-threads. Desktop arboard is unchanged. `keybinds.rs` assumed winit's desktop modifier supplement; UIKit now learns letter labels from logical keys, preserving default symbol labels because unmodified symbol text is unavailable. |
| `horizon-client-render` | `src/gfx.rs`: Renderer owns wgpu instance, window surface, GPU resources, shaders, uploads and frame presentation. Backend search previously tried Vulkan/OpenGL on iOS. Now selects Metal explicitly. Vulkan-only HDR hooks and GLES feature selection exclude iOS. `Renderer::new` takes prepared texture arrays/HUD/glint/panorama inputs, and `Frame` supplies the game render state. It is not a standalone game bootstrap. |
| `horizon-client-lib` | Manifest uses `horizon-client/src/client_lib.rs`, depends on render/input/world, and offers reusable systems without gameplay. Disable `audio`/`controllers` for the initial iOS graph. |
| `horizon-client` | `src/lib.rs`, `entry.rs`, `game.rs`: gameplay is already a library; executable is thin. `entry::main_with` parses process arguments and configures logs/allocator before `game::run_with`. The app must eventually supply explicit resource/game paths rather than invoke desktop defaults. `Game::resumed` creates a winit window; it currently returns immediately for an existing window and has no `suspended` callback, so the diagnostic lifecycle fix does not establish gameplay lifecycle correctness. |
| `horizon-prepare` | `src/lib.rs`, `java.rs`: Mac-side preparation runs Java/javac exporters and data generation. `Command` APIs compiling does not mean subprocesses or Java can run inside an iOS app. Keep this tool on the Mac. Manifest schema is 7; data version is 5023 (26.3). |
| Mods | `horizon-mods/Cargo.toml` is data formats/index validation, not a wasmtime runtime in this revision. `docs/native-mods.md` describes build-time native code injection/rebuilds. The diagnostic loads no native modules or runtime code. Future native mods must be compiled into the signed app, never downloaded and executed as arbitrary code. |

Minimal diagnostic graph: UIKit scene adapter → `horizon-ios` →
`horizon-assets` → `horizon-core`, plus `horizon-threads`, winit and wgpu/Metal.

Minimal real-world path: `horizon-client` → `horizon-client-lib` →
`horizon-client-render` + `horizon-client-input`, with `horizon-world`, assets,
core, items, command, protocol and supporting data crates. A library build of all
three client layers passes for arm64 iOS with `--no-default-features` after the
scoped fixes here. That establishes compilation, not linking or playing the
entire game on a phone.

## Native lifecycle and graphics

`ios/App/main.m` calls one exported Rust entry point with Foundation-resolved
Application Support, Caches and bundle paths. Swift is unnecessary for this
boundary. winit calls UIApplicationMain, owns the window/controller and handles
the event loop; there is no MTKView, second renderer loop, WebView or stream.

An actual simulator launch with only winit 0.30's legacy lifecycle trapped in
`___UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption_block_invoke`
on iOS 27. The app therefore declares a single UIWindowScene and supplies a
small UIWindowSceneDelegate. The scene adapter attaches winit's actual UIWindow
using its public UIView raw handle. No patched winit fork or private API is used.

The diagnostic selects only Metal, FIFO presentation, a 60 Hz CPU pacing target,
BGRA sRGB surface format selected through wgpu's supported default, Depth32Float,
and no optional GPU features. Resolution limits come from the adapter: plain
`downlevel_defaults` only allowed 2048 pixels and an actual high-density simulator
launch caught that error. Geometry is uploaded once; each frame writes one 64-byte
camera matrix. Depth allocation changes only with drawable resizing.

UIKit `inner_size` in winit 0.30 is the safe-area rectangle. This app uses
`outer_size` for its full-window Metal drawable; its localized UIKit diagnostic
label uses safe-area anchors. Touch displacement is divided by display scale.
Drag orbits the synthetic cube; this is **not** Minecraft camera/player input.

Suspension stops frame requests and releases the surface. Resume recreates it,
resizes depth/drawable state and clears the timing sample window. Surface
lost/outdated/suboptimal results are handled separately. Fatal graphics errors
stop drawing and show a localized error. Rust panics emit location/backtrace;
unwinding is not permitted across the C boundary. Device-loss recovery of all
GPU objects remains future work; a fatal device loss may terminate the app.

## Resource staging

`ios/scripts/pack-resources.py` takes the owner's Mac-prepared folder. It checks
schema/data version, rejects symlinks and executable/JAR/class payloads, excludes
the root-bound packed cache, and writes a private, gitignored bundle directory
with SHA-256 for every transferred file. It does not download or authorize assets.

The Rust importer validates paths, hashes, schema and data version before
copying. It copies into a staging directory inside Application Support and
renames only after verification. Content-addressed destination folders avoid
overwriting an earlier import; existing destinations are rechecked for corruption.
Missing/corrupt resources produce a visible message and detailed Xcode error;
they do not prevent the synthetic diagnostic from running. Transfer hashes prove
integrity relative to this local index, not ownership, authenticity or completeness
of every game registry. Actual `Assets::load` must perform semantic validation
before gameplay. Prepared data has since been imported during real-device testing; see the
follow-up validation.

`Application Support/HorizonIOSDiagnostic/saves` is reserved for future game
saves; `Caches/HorizonIOSDiagnostic` is created for disposable caches. Neither is
connected to gameplay yet. Import is development-only app-bundle staging; there
is no Files document picker or runtime download path in this milestone.

## Phase 2 implementation (initial implementation)

`horizon-ios` now depends on the reusable client library without audio/controllers.
`terrain.rs` loads `Assets`, `RenderAssets`, `BlockFacts`, `Biomes`, visibility and
`MeshTables`; `save::decode_lit` reads the explicitly supplied chunk, and `Mesher`
compiles its real sections. `gfx::Renderer::upload` receives the original packed
terrain vertices; `Renderer::render` uses the existing textured terrain/depth/light
passes. Environment lighting samples the prepared Overworld at noon. No mobile
terrain shader, imitation landscape or simulation was added. The diagnostic
shader remains solely as the optional synthetic fallback.

The chunk is read-only and isolated, without neighbor chunks. Boundary faces and
biome blending therefore lack neighbor data; animated block entities, entities,
clouds and gameplay are not initialized. Biome blending is disabled and the
mesher zoom seed is fixed to zero for this isolated inspection. This does not
claim exact seed-dependent biome boundaries. The camera starts above the central
column, outside its north edge. The renderer receives section indices relative
to the column minimum Y, as the desktop streaming path requires.

`flight.rs` supplies simultaneous move/look/vertical touches with point-scaled
coordinates, elapsed-time movement, pitch clamping and cancellation. UIKit draws
localized control labels and a sensitivity slider; every non-slider touch passes
to winit. This is free flight, not the player input abstraction.
`Renderer::suspend_surface` stops frame generation, waits for submission and
drops active/spare surfaces; resume uses its existing `recreate_surface`.
The previous `release_surface` is for renderer handoff and retains a spare, so
it is deliberately not used for UIKit suspension.

At initial implementation, matching resources were unavailable and only the
fallback could be tested. The follow-up validation at the end of this document
and the validation report supersedes that initial blocker.

## Follow-up plan after Phase 1 (historical)

1. Supply legally prepared 26.3 resources and a disposable saved chunk, then
   execute and visually verify the new terrain path, texture mapping, lighting,
   camera controls and suspend/resume. Measure real startup and memory before
   expanding scope. No terrain runtime result exists yet.
2. Extract/configure a mobile bootstrap for the gameplay library with explicit
   sandbox paths. Adapt its resume/suspend path and scene binding before running
   `Game` on iOS. Compilation alone does not validate its runtime assumptions.
3. Add touch actions through the existing `actions`/`keybinds` abstractions:
   movement, look, jump, attack/use, inventory, pause. Release held actions on
   cancellation/background. Keep simulation/physics unchanged. UIKit clipboard
   compiles but has not been exercised on the device. Audio is disabled and
   Game Controller input is unimplemented in this app.
4. Use small measured render/simulation distances, worker caps and optional
   internal resolution. The diagnostic is not a basis for world FPS predictions.
   Measure CPU/GPU work, frame intervals, memory, loading and long-run thermals
   with actual world content. No thermal or gameplay benchmark exists yet.

See [BUILDING.md](../BUILDING.md) for exact commands and
[ios-validation.md](ios-validation.md) for performed checks and limits.

## Minimal route from inspection to single player

Use `VanillaTerrain::for_world`/`new` (`horizon-client/src/vanillagen.rs`) with the
prepared registry, biomes, seed, dimension and generator tables. Let the existing
`streaming::Streaming` + `WorkerContext` own generation, neighbor light snapshots,
meshing and upload budgets rather than extending this static fixture loader into
a second chunk manager. This requires real resource/performance validation first.

Playable single player then belongs in `game/integrated.rs`: a serving `Game`
initializes dimensions in `levels.rs`, ticks simulation, and the window joins
through the existing memory client. Player state and guest actions stay in the
shared gameplay path (`lan_actions.rs`); movement/collision stay in `motion.rs`
and `collisions.rs`. Route mobile actions through client-input's interaction and
keymapping abstraction before replacing diagnostic flight. The next bootstrap
must provide explicit sandbox resource/game/options/cache paths and adapt the
full game's UIKit suspend/resume lifecycle. The inspection app currently links
reusable systems only, not `Game`, authentication or a simulation loop.

## Follow-up after the owner's Prism 26.3 installation

Matching client/assets are now available and Mac preparation passes `--check`.
The fixture tool can generate lit terrain through the real `VanillaTerrain` and
`Streaming` pipeline without opening saves. Mobile staging omits optional audio,
Mac exporter bytecode and both root-bound caches. Real execution exposed and
fixed a shared-renderer capability gap: flat draws require indirect execution in
addition to vertex storage/compute support. Adapters lacking it retain the
existing direct-draw terrain path. See the latest validation section for actual
runtime results; the missing-resource statements above describe the earlier run.

The final startup enters UIKit immediately and performs full transfer validation
and copy on a worker. This resolves the measured 20-second process-launch
watchdog failure from synchronous import. The main thread owns the handoff from
the loading renderer to the real renderer and all window/surface work. The worker
never operates UIKit or GPU objects. Asset/pipeline initialization at handoff
still stalls the main thread and needs a future asynchronous renderer bootstrap.
The signed bundle and private staging trees remain stable during verification;
each shared parent directory is checked once per pass, each leaf and hash in full.

Current next steps: neighboring chunks via the existing streaming coordinator,
representative lighting/biome boundaries, asynchronous GPU initialization, then
the integrated server/player/input/collision bootstrap. No simulation, collision,
entity rendering, world editing or full streaming is claimed by this milestone.

## Phase 3: native generation and camera-centered streaming (2026-11-07)

This section supersedes the static-fixture implementation above. The app now
creates an ephemeral seed-1234 Overworld with `VanillaTerrain`, `World`,
`WorkerContext` and `Streaming`. It never opens a save or uses `ios-terrain.nbt`.
The spawn origin is (-33,4). This remains diagnostic flight, without a player,
collision, entities, inventory, ticking block simulation or persistence.

`horizon-ios/src/lib.rs` owns explicit Uninitialized → LoadingResources →
PreparingWorld → UploadingGeometry → Ready / Failed states. Import and CPU
preparation each have a single named worker, sequentially. Initial registry,
texture/model reload, mesh/light tables, generator initialization, spawn search
and initial terrain lighting run off the UIKit thread. Finished jobs are joined
only after `is_finished`; completion while suspended waits for resume before GPU
setup. UIKit/window/surface/renderer creation stays on the main thread. The
synthetic loading scene continues presenting and accepting lifecycle callbacks.
Localized preparation/upload status distinguishes the two phases.

`horizon-ios/src/terrain.rs` configures two shared job workers, radius 2 by
default, clamped to 2–3 with `HORIZON_IOS_RADIUS`. Radius 1 was tested and rejected:
the scheduler's circular final-column envelope plus the mesher's requirement for
eight lit neighbors only meshes the center column. Radius 2 produces nine meshed
columns at rest (29 final columns plus intermediate generation/light halos).
The rendering footprint is smaller than the streaming view-distance predicate;
outer boundaries are visible at this intentionally small distance.

The camera's floored block coordinates are shifted by four, including negative
coordinates. The existing coordinator prioritizes distance, caps jobs in flight,
rejects outdated section sequences, lights neighboring snapshots and meshes
through the normal `Mesher`. Biome zoom uses `obfuscate_seed(1234)` and the normal
blend radius; translucent layer flags come from the real render assets. No
alternative terrain or lighting implementation was introduced. Visibility
ranking by camera direction is not added; normal renderer frustum culling still
applies. Neighbor snapshots allow internal face culling and blended biome/light
sampling; outer edges remain exposed. Water uses the existing translucent pass.

The main thread drains one coordinator batch at a time, at most two sections per
frame and stops after the first upload exceeding a 2 ms soft budget (an individual
upload is not preemptible). A pending batch prevents scheduling another batch;
there is no accumulating per-frame upload queue. Camera chunk changes revoke
pending uploads and re-dirty those sections before updating the coordinator.
CPU eviction uses the existing status/decoration halos; the returned removed
columns drive `Renderer::remove_columns` and geometry-accounting removal. Mesh
buffers return to the coordinator's capped pool. Sprite pixels use the existing
idempotent background `pack` operation after GPU upload; generator scratch/shared
caches use the scheduler's existing idle-release behavior. Continuous movement
still needs longer memory/thermal testing; a bounded view is not proof of a
constant resident working set.

The importer handles individual prepared files, not a compressed archive. Its
cost is tens of thousands of metadata operations, SHA-256 reads and copies;
texture/model decoding happens later. Warm startup now verifies the existing
content-addressed destination against the bundle's index without rehashing the
unused source payload. Every imported file, safe path, schema and data version
is still verified. Cold publication retains source validation, staging copy,
staging re-verification and atomic rename. Interrupted staging is removed on the
next cold attempt; prior imports and saves are preserved. No cold-import speedup
is claimed. Logs separately time verification, copy and publication verification.
The source index and mutable sprite-cache exclusions retain the existing cache
invalidation contract.

### Integrated-server dependency map and next implementation

| Responsibility | Actual entry point / dependency | Mobile work still required |
| --- | --- | --- |
| Start / stop | `horizon-client/src/game/integrated.rs`: `Game::open_integrated`, `serve`, `Integrated::request_stop`, `stopped` | These are gameplay-module-private and require a fully bootstrapped `Game`; expose a scoped mobile bootstrap, not a synthetic tick loop. |
| World / save ownership | `game/world_loading.rs`: `WorldRequest`, `WorldLoader`, `PreparedWorld` (session lock, SaveDir, Level, World, Streaming); `game/levels.rs`: dimension parking/`enter_level` | Supply sandbox-owned disposable game directory and explicit resources/options/cache paths before any initialization. Desktop code still resolves some saves from process cwd. |
| Tick schedule / lifecycle | `integrated.rs`: `run_integrated_inner`; `levels.rs`: `tick_server_timed`, `wait_for_next_server_tick` | Real 20 Hz thread with pause/save semantics; pause on UIKit suspension and handle asynchronous stop/save within iOS background limits. |
| Local player session | `integrated.rs`: `poll_integrated_start`; `game/remote.rs`: `connect_local`; reusable `net.rs`: `Client::connect_local` | Adopt the returned `horizon-protocol::connection::Pipe`; keep the normal login/configuration path and owner's profile handling. No TCP listener is required (`NetConfig.listen=false`). |
| Entities / chunks | `game/remote.rs`: `handle_packet`, `remote_chunk`, `tick_remote`; `lan/tracking.rs`, `lan/chunks.rs` | Use actual entity and `LevelChunkWithLight`/`ForgetLevelChunk` packets. Replace diagnostic generation ownership with server-owned generation and the existing remote streamer. |
| Actions / inventory | `game/lan_actions.rs`: `as_guest`, `tick_guests`; `game/hand.rs`, `game/stations.rs`, reusable `menu.rs`; client-input `interaction_input.rs` | Map touch actions into existing bindings and packets, release held actions on cancellation. Flight must not move the server player. |
| Physics / validation | reusable `motion.rs`, `motion/`, `collisions.rs`; `game/lan_actions.rs`, `lan/guest.rs`, `horizon-world/player_travel.rs` | Keep existing simulation and movement checks; replace flight with the player session's movement state. |

The inspected integrated startup, ticking and memory transport use native Rust
threads, channels and signed compiled code. No JIT, executable-memory allocation,
Java or dynamic module loading is required by those paths. This is a source
assessment, not an on-device integrated-server execution result. Preparation
stays on the Mac. Desktop native-mod build/injection facilities must not become
an iOS runtime module-loader path. Audio, full gameplay resources, platform
lifecycle and save-path assumptions remain unvalidated for a full `Game`.

The shortest next slice is a mobile entry in the gameplay crate that constructs
`Game` with explicit sandbox paths and static/no-runtime-mod configuration,
opens one disposable world through `open_integrated`, adopts its memory Pipe,
and lets `tick_remote` feed the existing renderer/streamer. Reuse this phase's
UIKit scene/surface lifecycle and worker preparation. Then map touch move/look,
jump and attack/use to existing actions, initialize the actual player/collision
state and validate a block interaction. The inspector cannot safely instantiate
only `Integrated` in isolation: it depends on private `Game`, `Resources`,
`PackResources`, world/session and guest initialization. That extraction exceeds
this rendering slice; no dummy server connection has been added.

## Phase 4: native gameplay entry (2026-11-07)

Baseline remains `18840db9146b2521dd46ad52a4559a49be180f8f` (`upstream/main`);
there is no `origin` remote. Earlier uncommitted port work was preserved.
Read-only `gh pr list --state open --search ios` could not reach GitHub. No
issue/comment/PR, commit, push or publication was performed.

The default scene now enters `horizon-client::game::mobile::Session`, which
embeds the upstream loading `App` in the existing UIKit/winit loop and adopts
its existing window. `game.rs::new_loading` is the ordinary desktop loading
constructor extracted without changing its desktop behavior. It decodes the
complete gameplay resource bundle on upstream workers; no diagnostic terrain
registries or generator are initialized alongside it. The small loading renderer
hands its device to the ordinary full world renderer. The resource importer
retains its existing full integrity checks.

`Loading::finish` constructs the real client `Game`; `StartupMenu::handoff`
transfers its pending local-world entry. Normal world entry invokes
`Game::open_integrated`, `serve`/`server_game`, `WorldLoader`, the Overworld and
`poll_integrated_start`/`connect_local`. `NetConfig.listen=false` retains the
memory `Pipe` login/configuration path without a local TCP listener. The
existing remote packet, player motion/collision, block interaction, meshing,
chunk eviction, entity and save code owns gameplay. No mobile simulation,
chunk generator, player state, authentication bypass or save format was added.

Temporary controls use per-touch ownership in `horizon-ios/src/gameplay_input.rs`:
left lower stick, right look, jump/break/place on the right edge, pick-block
beside place, inventory/pause at the top. Edges feed `mapped_key` and
`mouse_mapping`, which feed upstream movement and interaction. Pick-block uses
the actual Creative action to select a placement item. GUI touches become a
single owned upstream pointer; another finger cannot release it. Cancellation,
resize, focus loss and screen changes release held touch actions. iOS capture
is logical because UIKit cannot grab a desktop mouse cursor. Native labels use
Localizable.strings; menus, inventory, loading and save/exit remain upstream UI.

Paths are established before spawning workers: Application Support/
`HorizonIOSDiagnostic/Gameplay`, containing the actual `saves/Mobile Creative`
world, options and derived caches. Seed 1234/Creative apply to creation; existing
world data is loaded by upstream. The importer remains in its separate
content-addressed resources directory. Test sessions use a validated leaf under
`Tests/`, never the regular gameplay world. Runtime mods are disabled in the
mobile settings; no launcher, signals, desktop process bootstrap or dynamic
module loader is invoked. Audio/controllers remain compiled out.

Suspension releases touch actions and the Metal surface, opens the existing
pause menu, requests integrated-server pause, and queues `save_game` plus
`SaveDir::flush` on the simulation thread. UIKit grants bounded background time;
the event loop polls an explicit save receipt and ends the background task on
completion. A server still opening is paused through its starting handle.
Resumption recreates the surface and retains the pause screen. Save/exit uses
the ordinary asynchronous closing-save path. Expiry logs an unconfirmed save;
sudden termination still cannot guarantee completion. Recovery and repeated
lifecycle behavior require runtime checks, not just compilation.

`MOBILE_SERVER` samples executed active simulation ticks, elapsed wall time,
mean/max tick duration and 50 ms overruns; it resets across pauses. These are
engine tick measurements, not an independent timer. `MOBILE_CLIENT` logs actual
player position/ground state, FPS, loaded columns and sampled resident memory.
`MOBILE first_playable_ms` waits for the real Game's first drawn world frame;
it starts after resource import. Full launch time must also include the importer.
Detailed frame distributions remain available through upstream control `stats raw`.
See the Phase 4 section of [ios-validation.md](ios-validation.md) for observed
results and the distinction between compiled and runtime-verified behavior.

The first physical handoff exceeded UIKit's ~1 MiB stack. Heap-pinning the
renderer future alone was insufficient: the compiled loading callback retained
about 269 KiB, `Loading::finish` 575 KiB and renderer setup/poll another 273 KiB
at once. Startup now keeps its Game boxed through `start_game` and loading
handoff, then constructs the desktop split wrapper in a separate non-inlined
frame after loading returns. The existing splitscreen guest bootstrap consumes
the same boxed result. This changes private startup ownership, not Game state,
thread/transport ownership or gameplay. Validation tracks both failed attempts
and the boxed-handoff result.

Final Phase 4 outcome: the boxed startup runs on the iPhone 13 mini. Its real
integrated server logs in the owner through memory, renders server chunks and
records roughly 20 active TPS. Actual phone owner data/regions were saved; a
fresh process restored the non-spawn saved position exactly. Saved stats record
5.67 m walked. The complete action/persistence route passes on the simulator,
including server-confirmed break/place and remeshing at an X/Z chunk corner.
Both target Release builds, 1,109 unit tests, desktop LAN and the three vanilla
server harnesses passed (see validation for timing/order and environment retries).
The owner subsequently confirmed that walking/collision, holding Jump, and
breaking/placing all work through direct touch on the iPhone. This is
user-confirmed physical validation, separate from the automated simulator checks;
Device Hub's failed drag actions were not counted. Modified-block persistence
after a phone save/relaunch, long play, interrupted-save recovery and controlled
phone frame/HUD measurements remain unverified. The measured 1.236 s phone
handoff is still a main-thread responsiveness limitation. No fully physically
validated 60 FPS claim is made.

## Phase 4.1 input architecture

`ios/App/main.m` owns the accessible Back/Chat buttons and the UIKit text
responder, attached to winit's actual scene/window. It does not choose game
screens. `crates/horizon-ios/src/native_input.rs` queues owned UTF-8 text,
UTF-16 selections and action requests for the normal game-loop thread;
`game/mobile.rs` resolves focus identities and invokes existing actions.
`game/native_text_field.rs` adapts the real text boxes, with specialized
transactions for signs, book pages, dialogs, block editors and world options.
All desktop keyboard/event paths remain in place; UIKit is used by the iOS host.
The Xcode app target supports iPhone and iPad; floating keyboards use the public
keyboard layout guide. No renderer or networking replacement is involved.
