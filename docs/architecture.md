# Horizon iOS architecture

Horizon iOS applies a patch series and copies an overlay onto a pinned [Horizon](https://github.com/horizon-rust/horizon) revision. Horizon provides gameplay, world generation, saves, networking, and the wgpu renderer. The iOS port supplies the application host, Metal integration, and mobile input and audio. See [patch layout](../patches/README.md) for the source reconstruction.

| Path in the reconstructed tree | Responsibility |
| --- | --- |
| `ios/App/Launcher.swift` | SwiftUI launcher, Files picker, offline profile, instance selection |
| `ios/App/main.m` | UIKit host, window and lifecycle bridge, keyboard, touch HUD, audio session |
| `crates/horizon-ios/src/` | Rust application entry, resource import, instance storage, touch gestures, native input bridge |
| `crates/horizon-client/src/game/mobile.rs` | Game sessions, input dispatch, pause and save, return to launcher |
| `crates/horizon-client-render/src/audio/ios_stream.rs` | Bounded streaming decoder and PCM buffers |

## Sessions and storage

Play opens Horizon’s title screen with the selected instance as its game directory. Singleplayer joins the integrated server on its own thread over an in-memory connection. Direct Connection uses Horizon’s Java 26.3 network client (protocol 777). On suspension, the host releases held inputs and the Metal surface and requests a save and flush within UIKit’s background allowance. Resume restores the surface and leaves the game paused.

Instances have independent saves, settings, and packs. The offline profile, touch layouts, and prepared resources are shared. Import checks the staged files against their SHA-256 index before making the private copy active. Warm launch checks the active index and required files. Resource tables are initialized for the process, so replacing the import after Play requires an app restart. See [instance storage](instances.md).

## Input and display

winit owns the UIKit window and delivers world touches. The Rust host dispatches movement, camera, and game actions while UIKit draws the touch controls. The keyboard forwards text and UTF-16 selection to Horizon’s focused field. An explicit tap or Chat request opens the software keyboard, with a frame-notification layout fallback on iOS 15.

Devices on iOS 15 or with at most 2 GiB of RAM use the constrained rendering path. It limits work in flight, reduces mesh and upload budgets, and skips optional graphics paths. Fresh constrained instances use a 60 FPS cap and 50% scene resolution. The cap is a setting. Display-paced presentation and the drawable pool limit frame stalls. Startup prioritizes the title screen and moves asset work off the main thread where possible.

## Audio

Horizon’s mixer and sound registry read the imported Ogg Vorbis assets. CPAL sends output to Core Audio, with an `AVAudioSession` bridge for routes, interruptions, and foreground changes. Music decodes incrementally into bounded buffers. Audio stops on suspension. Volume settings are per instance under **Options → Music & Sounds**.
