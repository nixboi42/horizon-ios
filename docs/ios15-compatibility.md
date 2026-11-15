# iOS 15 / 2 GB compatibility, after 0.2.0 build 6

The changes are in the `launcher-dev` candidate stack, patch 10 and its active
overlay. The locked build, frozen snapshots, TestFlight scheme, bundle and signing
configuration, instance storage and save paths are unchanged.

## What the reports establish

The supplied iPhone8,2 reports put the abort inside AGX/IOGPU during Metal command
buffer creation. In wgpu 30.0.1, render pass replay closes the previous HAL
command buffer and opens another with the current pass label before it calls
Metal's render encoder creation. The stack at `begin_encoding` therefore does
not identify the pass or show that Metal had received its attachment descriptor.
The five VM shortage entries and live resource loaders make memory pressure a
strong lead, but neither prove an out-of-memory termination nor exclude an AGX
driver failure. The reports do not quantify the peak.

Compared the exact build 6 reconstruction with the locked v0.1.0 source
(snapshot `3b17e3f6ad57`). They share the upstream revision and Metal command
buffer code. Early title GPU work and asynchronous world-tail loading are
already present in v0.1.0. Build 6's startup budget work serializes the handoff
model tasks and defers the prepared cache writer, but it still overlaps UI GPU
work and renderer handoff with the resource tail. The code comparison does not
show a new Metal backend or establish that a recent startup optimization caused
the abort. The earlier successful A9 iPad run used iPadOS 16.7.16; that is not an
iOS 15 driver control.

A9 is Apple GPU family Apple3. The existing adapter checks are still in use,
including per-device features and format checks and iOS 15's Metal Shading
Language 2.4 selection. The compatibility path uses an SDR target, bypasses the
optional wide/float frame and HUD coverage paths, and starts with shader packs,
temporal history, frame interpolation and GPU/entity occlusion disabled. These
are reductions in attachment and feature exposure, not a claim that a specific
A9 format or pass is defective.

## Budgets and expected tradeoffs

- Devices reporting at most 2 GiB and devices on iOS 15 use the compatibility
  path. It starts the world renderer after the resource worker has finished.
  CPU resource stages run serially on the loader and initial world preload
  does not compete with them. SwiftUI shows a loading indicator while the CPU
  work finishes. The title panorama loads on the same worker after the GUI atlas.
  The optional prepared data writer is skipped.
- The initial viewport uses native UIKit scale and full scene resolution,
  30 fps, 2 chunks on a fresh instance (saved startup values capped at 4),
  and simulation distance 5. Block sprites use the 32 MiB top-mip budget;
  this is not a cap on all textures. The shared RGBA atlas still has its
  original dimensions and CPU data, but its GPU upload stages one strip
  of roughly 4 MiB at a time. Metal retains two drawables and the serialized
  path waits for GPU completion between frames.
- Meshing uses one shared worker and permits eight sections counting in-progress
  meshes and the upload backlog. Deferred sections remain dirty. Startup
  uploads target four sections and 1 MiB per frame; a single larger mesh must
  still be allowed through to make progress. Section staging uses a 1 MiB
  allocation unit and terrain arena pages are capped at 16 MiB each. These
  are limits on batches and individual pages, not hard caps on all terrain.
- On a memory warning, completed pending meshes can be discarded and remeshed;
  the renderer drains submitted work with a timeout before clearing staging
  belts and retired pages. The allocator returns unused pages. Live world and
  resource data remain resident. The upstream handoff also still pins its
  shared resource owner for the lifetime of the process, so repeated launcher
  re-entry needs its own memory soak test.
- Larger devices on newer iOS keep their normal startup concurrency, render
  settings and queue behavior.

The keyboard bridge now uses the game window passed by winit for the launcher
and text responder rather than discovering an unrelated scene window. The scene
activation guard handles legacy windows as well. A one-shot tap or Chat request
is retained by field identity until UIKit can activate that editor in the key
window. iOS 15 uses keyboard frame notifications for layout and re-enters first
responder when switching from the silent hardware-editor input view to the
software keyboard. Entering a screen with a focused field still does not request
the software keyboard. The existing Rust field identity, UTF-16 selection and
marked-text handling remain in place.

## Device capture still required

There was no connected iOS 15 or A9 device during this work. No physical
footprint measurements, crash reproduction, or pass identification are available.
An iOS 27 simulator cannot confirm the AGXMetalA9 or iOS 15 UIKit behavior.

On the iPhone 6s Plus, run build 6 and the candidate in disposable instances
with the same imported resources and world seed. Capture `MOBILE_MEMORY`
through startup, title, first world generation, first playable frame and 60
seconds of gameplay. `phys_footprint_bytes` is the current task footprint;
`lifetime_peak_bytes` is the kernel's process high-water mark, not a
phase-reset counter. A zero value is unavailable. Use Instruments VM Tracker
for isolated phase peaks. A separate run with `HORIZON_IOS_METAL_TRACE=1`
prints the native pass label before command-buffer and render-encoder creation,
the unsubmitted buffer count, attachment summary, and Metal device allocation.
The Metal allocation counter overlaps unified memory accounting; don't add it
to the task footprint. Disable tracing for timing comparisons.

Check normal field navigation without a keyboard, explicit field taps and
reopening after Done, Chat opening immediately, composition and selection,
background/resume, an existing save, and return to launcher/re-Play. Repeat on
the A9 iPad with the earlier build as a reference, and on a newer device to
check the normal path.

Local checks: the candidate reconstructed offline without dependency changes;
the arm64 iOS target passed Rust type-checking at deployment target 15.0.
The Objective-C bridge passed iOS 15 availability checks and the Swift
launcher passed type-checking in both ordinary and TestFlight configurations.
The builder and packaged-source Python suites passed (8 and 11 tests).
The Rust host suite passed 21 tests, with one ignored.
The UTF-16 text bridge test and both resource-backed mobile session tests
also passed on the host. The latter used fresh scratch game directories.

The local unsigned Xcode Release build linked and verified arm64 and a 15.0
minimum in both Mach-O and Info.plist. It started compiling before the final
mesh-accounting adjustment; that adjustment passed a separate arm64 Rust check.
A temporary copy stripped using the existing packaging step passed the release
privacy scan. No signing, installation, archive upload or publication was done.

The refreshed preview checks the active resource index and required files on warm launch. It skips the per-file scan of the already imported private copy. New imports still verify the staged copy before publication.
