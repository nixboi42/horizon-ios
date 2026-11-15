# iOS startup work on launcher-dev

The candidate series (`patches/next/series`, patch 08 and its overlay) contains this
work. The locked release and frozen snapshots are unchanged. This is a static-only
change: no compilation, app launch, benchmark, or measured performance improvement.

## Pipeline and changes

| Stage | Finding and treatment |
| --- | --- |
| Native launch → SwiftUI | UIKit already starts before the import worker completes. Instance catalog operations already use a serial queue. Added host-entry, SwiftUI appearance and usable-launcher timing; usable means resources ready and a selected catalog entry available. |
| First import | Previously read/hash source, copy source, then read/hash staging. Now validate source layout/schema first, copy, then fully SHA-256 verify staging before atomic publication. Invalid bytes cannot become active; a corrupt import can now consume copy time before rejection. |
| Warm verification | Previously rehashed every indexed byte on every launch. A disposable private receipt now binds the index digest, vanilla/data version, and each file's device/inode/length/mtime/ctime (including nanoseconds). Every warm access still walks indexed paths, rejects symlinks and checks manifest compatibility. A missing/torn/stale receipt triggers full hashing; a changed payload must still match its expected hash. Non-Unix platforms never take the metadata shortcut. |
| Shared import | Initial bundle/Documents fallback now records the active ID just like picker import. Later launches can use the private verified copy even if the external source is removed. Receipts and prepared caches belong to that shared content-addressed root, not an instance. Resource replacement still requires restart after a session has started because upstream tables are process-wide. |
| Play preflight | Moved resource completeness checks, datafix resource registration and native model/component registration into the existing resource-loader worker, before any dependent workers start. Hook installation, settings and UIKit/Metal surface ownership remain on the main thread. |
| Asset/model CPU work | Existing loader already prepares mipmaps, entity documents and gameplay tables off-thread; resources are shared with the integrated server through existing ownership. Ten concurrent GameCpuResources model tasks now execute sequentially on the existing loader on iOS, reducing overlapping allocations/thread stacks. Desktop parallel behavior remains. No additional decoded asset cache is retained between sessions. |
| Optional cache generation | `horizon-data.bin` creation was started while first-world preparation was still active. On iOS only its path is retained until the first playable frame. Cache lookup and writing run on the existing idle-priority writer; a try-lock prevents overlapping writers across sessions. Existing upstream cache validation stays in force. |
| Chunk generation/meshing | Existing distance-ranked submission, world preload, shared worker pool and first-install render distance 2 remain. No world-generation order, simulation distance, save format or multiplayer behavior changes. Extra parallel generation would be risky on a 2 GB iPad. |
| Chunk GPU uploads | Existing nearest-first pending uploads and frame budgets progressively populate terrain. These remain unchanged, as do textures, geometry quality and steady-state budgets. |
| Renderer/shaders | Boot device creation and world renderer/pipeline handoff still run synchronously on the main thread. Existing trace stages measure pipeline creation and texture uploads; a separate mobile boot-renderer duration was added. This remains a likely large responsiveness bottleneck, requiring device-level validation before changing ownership. |

The native model stage trades concurrency for a smaller temporary working set;
it may take longer on devices with more cores. Deferred cache I/O can compete with
terrain arriving after control becomes available. Measure both startup and that
following interval. Metadata receipts assume the app-private store is not under
hostile concurrent mutation; they are transfer-integrity optimizations, not an
authenticity check or protection against silent bit rot with unchanged metadata.
Deleting a receipt forces full re-verification. No receipt is trusted from an
external resource folder.

## Timing semantics

Normal console output includes:

- `MOBILE launcher_visible_ms`: SwiftUI `onAppear`, relative to native host entry;
  a UI lifecycle marker, not proof of physical display scanout or process creation.
- `MOBILE launcher_usable_ms`: resources and initial selected catalog entry ready.
- `MOBILE resources_ready_ms`: resource worker completion observed by the host.
- `RESOURCE_IMPORT source_layout_ms`, `copy_ms`, `publication_verify_ms` and
  `RESOURCE_VERIFY cache=hit|miss elapsed_ms`: import/verification stages.
- `MOBILE preflight_ms`, `boot_renderer_ms`: individual stage durations.
- `MOBILE first_rendered_ms`: first successful loading-frame render, measured from
  Session construction. Failed/skipped presents no longer count as shown.
- `MOBILE first_world_frame_ms`: first observed successfully rendered world frame.
- `MOBILE first_playable_ms`: first update satisfying the touch-control predicate
  (player, remote session, world frame, no loading overlay/save, not suspended).
  The old timer only checked `world_frames > 0`.
- `MOBILE play_to_controls_ms`: Play tap through serial queue/session setup to the
  SwiftUI notification enabling gameplay controls. This covers queue delay that
  the Session-based durations omit. Play now opens the main menu, so this also includes
  time spent choosing a world or server. It is logged once per Play request that enters play.

Set `HORIZON_IOS_STARTUP_TRACE=1` in a **later authorized device run** to enable the
existing fine-grained startup trace at the sandbox logging home's
`startup-trace.jsonl`. It covers core tables, atlas/panorama, models, renderer
pipelines, uploads, server/world entry and the new mobile marks. It is off by
default; the file is replaced on each process launch, so export it after each run.
Console metrics alone cannot establish peak memory or GPU completion latency.

## Later validation (not run)

Use disposable instances and `HORIZON_IOS_TEST_SESSION=startup-bench` for device
experiments; preserve real worlds and keep resource imports synthetic or owned.
Compare the previous candidate with this candidate on iPad 5 first, using identical
resources, settings, seed, thermal state and display conditions. Record at least
five runs for each: first import/new world, process restart/existing world, return
to launcher/re-Play, second instance with the shared import, and multiplayer join.
Capture memory/thermal data and frame times during loading and for 60 seconds after
controls enable. Warm-launch comparisons must use the same generated caches.

Once exported logs and traces are available, these commands only analyze files:

```sh
rg 'MOBILE .*(_ms=|CLIENT)|RESOURCE_(IMPORT|VERIFY)|startup handoff|core-ready|rest-ready' device-console.log
python3 - <<'PY'
import json
from pathlib import Path
# Wrap the exported JSON-lines trace for Perfetto; ignore a torn final line.
events = []
for line in Path('startup-trace.jsonl').read_text().splitlines():
    try:
        events.append(json.loads(line))
    except json.JSONDecodeError:
        pass
Path('startup-trace.json').write_text(json.dumps(events))
PY
```

In a prepared upstream checkout, the existing desktop comparison harness can also
be run **later**, with its automatically isolated scratch homes (not an iPad result):

```sh
python3 tools/startup_trace.py --world T --seed 1234 --runs 5 --keep cold.json
python3 tools/startup_trace.py --world T --seed 1234 --runs 5 --warm --keep warm.json
```

Before acceptance, compile/type-check and run the Rust resource tests, including
same-size edits with restored mtime, missing/torn receipts, symlink replacement,
failed import rollback and warm import without the external source. These tests
were added/retained but not run here. Exercise suspension during load, return and
re-Play, instance rename/delete/duplicate, resource replacement/restart, keyboard
navigation, camera controls, saved-world reopening and multiplayer. Verify the
first playable marker against video/input rather than treating a log as proof.

## Static validation performed

- Reconstructed the pinned upstream revision locally and applied the entire
  candidate series in order; patch 08 passes `git apply --check`.
- Parsed/formatted changed Rust with standalone `rustfmt` (no Cargo invocation).
- Inspected Swift bridge symbols, status keys and lifecycle predicates.
- No Rust tests, type-checking, shader compilation, Xcode, simulator, IPA generation,
  dependency installation, GitHub actions, publication, or performance runs.
