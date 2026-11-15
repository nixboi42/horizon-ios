# iOS audio on launcher-dev

The candidate enables Horizon's existing audio engine using CPAL 0.16's iOS
RemoteIO/Core Audio output. It targets iOS 15.0. No Minecraft sounds or other
proprietary assets are included in the app. Import prepared resources **with
sounds** from an owned Minecraft installation; a previous `--without-sounds`
import cannot produce audio until replaced with a complete import.

## Architecture

`horizon-client-render/src/sound_events.rs` reads the imported `sounds.json`:
weighted variants, nested event references, volume, pitch, attenuation distance,
stream flags and subtitles. Startup reads the registry on the resource loader
worker. Existing resource-pack merging and reload handle namespace overrides,
`replace`, and merged definitions; the audio patch does not implement a second
Minecraft event resolver. Pack reload discards the old decoder/cache pair.

`SoundEngine` retains upstream category, entity-following, delayed, looping,
pause and stop behavior. The shared mixer retains mono positional attenuation,
listener orientation and stereo panning. Stereo music/records play directly,
as in the existing OpenAL-style mixer. Symphonia decodes Ogg Vorbis. No new
codec dependency, Java runtime, JIT, microphone permission or background-audio
entitlement is needed.

CPAL already supplies the native iOS backend. The small `AVAudioSession` bridge
in `ios/App/main.m`, through `horizon-threads/src/ios_audio.rs`, owns activation,
interruptions, foreground transitions, system routes (wired/Bluetooth audio),
and media-services resets. A generation counter makes the Rust output reopen
after changes even though CPAL reports a constant iOS device name. The session
uses the playback category, including when the silent switch is on. Output is
released on suspension. Quit Game drops the output, session observers, voices
and cache; decoder workers observe cancellation and exit. Play creates a fresh
output and session. The app does not play audio in the background.

## Sounds and settings

The port uses the existing gameplay and network dispatch for block breaking and
placement, material footsteps, landing/fall damage, entities, weapons/combat,
weather/thunder, explosions, UI clicks, records, background music and biome/cave/
underwater ambience. Vanilla-defined jump sounds (for example mounts) are
retained; no generic player-jump sound is invented where Java has none.
Single-player already joins its integrated server through the multiplayer
client path, so it receives the same positional/entity sound handling.

Use Minecraft's in-game **Options → Music & Sounds** page for master and
individual source-category volumes, with the existing translated labels and
previews. Values use the instance's existing `options.txt` persistence. Category
semantics, track selection and music scheduling remain upstream's.

## A9 and 2 GB budgets

| Resource | iOS bound |
| --- | --- |
| Mixer channels | 48 total; ordinary allocation stops at 44, reserving room for streams |
| Streaming decoders | At most 4 |
| Decoded static cache | 12 MiB, LRU; at most 1,024 entries |
| Attached static PCM | 16 MiB conservative sum across voices |
| Single static decode | 2 MiB PCM; longer files fall back to streaming |
| Pending static decode requests | 48, nonblocking enqueue; stale jobs skipped |
| Music PCM ring | 32,768 frames per stream, at most 256 KiB stereo |

A single serial worker decodes cache misses. Stream workers decode packets
incrementally and wait when their rings are full; whole tracks are never retained
as PCM. The audio callback performs no filesystem access or decoding, uses
nonblocking locks, and reuses output/voice buffers. Stream underruns output
silence without losing the next sample. Memory warnings clear the static cache.
The above are PCM/queue limits, not a claim about total app RSS: codec state,
thread stacks, event metadata, allocator overhead and the rest of the game are
additional. Saturated budgets drop new sounds rather than grow without limit.
No renderer, terrain, touch-control or frame-pacing algorithms change.

## Validation and remaining gaps

Automated checks use generated synthetic audio, never bundled Minecraft audio.
Tests cover interpolation, stereo gains, underrun recovery, actual incremental
Vorbis decoding, a full bounded ring and cancellation. The native bridge is
compiled against the iOS 15.0 deployment target. Upstream event/mixer/settings
checks and the Release artifact checks are recorded with the build.

No physical-device audio test or A9 CPU/RSS measurement is claimed. Bluetooth
route transitions, incoming-call/Siri interruptions, speaker/headphone balance,
long music playback, and repeated Quit Game → Play cycles need manual listening.
This implementation inherits upstream event coverage; it does not establish
complete Java audio parity. It has stereo panning rather than HRTF/occlusion.
Only the existing Ogg Vorbis codec is enabled; arbitrary WAV/MP3 pack extensions
are not added. An interrupted or reset output may underrun while reopening.

For manual acceptance, import a full resource set, exercise the listed actions
in single-player and on a server, rotate relative to a stationary sound, change
and reopen category settings, switch routes during music, background/foreground,
interrupt playback, and repeat Quit Game → Play. Check an A9/2 GB device for
sustained frame rate and memory growth. Keep existing saves backed up through
the normal user workflow. The build process does not install or launch on a
physical device and does not change TestFlight signing or upload anything.
