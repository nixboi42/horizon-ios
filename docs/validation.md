# Downstream validation

## Source preservation

Upstream base `18840db9146b2521dd46ad52a4559a49be180f8f` matched the original checkout's HEAD and cached upstream/main. The local preservation archive and binary Git diff are in the original checkout's `.local/downstream-preservation/`. The extraction verified all 91 modified or new source files against the original contents and executable bits. The only excluded untracked file was Xcode's generated specifications cache. [Baseline manifest](baseline-source.json)

The downstream changes after that comparison are the shared deployment configuration, release and packaging automation, version-aware resource staging, and the iOS crash-report prefix. Release packaging removes linker debug-map paths from the staged executable. The game, Metal, mobile-input and UIKit source from the reconstruction are otherwise preserved.

## iOS deployment and GPU audit

- Xcode 27.2 and its iPhoneOS 27.2 SDK support iOS 15 deployment. Its simulator and device-debugging support begin at iOS 17. Rust 1.99.1 reports 10.0 as its device-target floor and 15.0 with the build override. The iOS library and Xcode use the same deployment value. [Apple Xcode support](https://developer.apple.com/xcode/system-requirements) · [Rust iOS targets](https://doc.rust-lang.org/stable/rustc/platform-support/apple-ios.html)
- The Objective-C application host compiled with the iOS 15 SDK availability checks promoted to errors. Its UIKit keyboard layout guide is available starting in iOS 15. The port contains no Swift sources. The Rust host's native text and safe-area calls go through the Objective-C bridge. Winit 0.30.13's iOS platform uses public UIKit event-loop, window and touch APIs. The vendored wgpu-hal Metal adapter checks OS and device capabilities, including an iOS 15 branch for Metal Shading Language 2.4. [UIKit keyboard layout guide](https://developer.apple.com/documentation/uikit/uiview/keyboardlayoutguide)
- Apple A8 is Metal Apple2. A9 and A10 are Apple3, which adds capabilities such as indirect execution and compressed volume textures. Horizon retains the adapter checks for indirect execution, compute, vertex storage, and render-target formats. Devices with missing capabilities use the existing world rendering and postprocessing fallbacks. A8 rendering still needs physical validation. [Apple Metal feature tables](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf)
- The iOS 15 arm64 Release app compiled and linked. The inspected IPA's executable records `LC_BUILD_VERSION` platform iOS and `minos 15.0`. Its plist records `MinimumOSVersion=15.0`. No dependency downgrades or availability shims were needed.

## Tests and device coverage

- The exact iOS 16 reconstruction built with Xcode Release. Its host crate passed 15 unit tests with one resource-dependent terrain test ignored. Both arm64 Cargo compilation checks passed. The three mobile regression tests passed, including the integrated session and in-memory remote protocol tests. The original iOS Python suite passed all 11 tests. The downstream builder's four fixture tests and six existing renderer capability and flat-path tests passed.
- Fresh Minecraft preparation used the selected version's pinned client and server files with Java 25. The resulting files passed the official preparer's completeness check. The mobile packer staged 23,219 files in the optional silent transfer.
- The iOS 15-target IPA installed in a separate app container on an iPhone 13 mini running iOS 27.2.1. The app imported 23,219 files from Documents, initialized Metal and reached the integrated world. The installed Horizon app and its save container were left intact. No iOS 15 device or simulator was available. The existing port was previously validated on iPad 5 (A9, 2 GB RAM) on iPadOS 16.7.16. That iPad was detected over USB, but the local development profiles did not cover it for an isolated test install.

- The iOS 15-target arm64 simulator Release built and reached the integrated world on iOS 27. All three existing simulator navigation tests passed, covering Back, pause, inventory, native chat editing and keyboard focus through background/resume. The Unicode paste and editing test passed. The address-persistence UI test failed twice because UIKit did not expose the Paste menu during the test. The mobile remote-protocol test passed using an in-memory connection.

## Build and physical coverage

| Measurement | Verified result |
| --- | --- |
| Lowest compiled deployment target | iOS 15.0, arm64 Release |
| Lowest linked and packaged deployment target | iOS 15.0, arm64 device IPA |
| Lowest OS physically verified in this run | iOS 27.2.1 |
| Oldest hardware physically verified in this run | iPhone 13 mini, A15 |
| Prior port validation | iPad 5, A9, iPadOS 16.7.16 |

## Latest upstream

The official default branch advanced during validation. Both `298c01b0206d72294b25753c9f8d0a77b742b3bb` and the final observed HEAD `d3bd732245543d66f4a1cd672862700f38844fb8` failed the same checks on November 13, 2026. Three-way application stopped at patch 01 with conflicts in `Cargo.lock`, `crates/horizon-client-input/Cargo.toml`, `crates/horizon-hdr/src/lib.rs` and `crates/horizon-threads/src/lib.rs`. The remaining patches and compilation require a reviewed rebase. The compatibility lock remains on the original base.

## Reproduce the baseline comparison

Use the original checkout's local archive and manifest from `.local/downstream-preservation/`. In a disposable clone, check out the commit named by the manifest, apply patches 01 through 05 in series order with `git apply --index --3way`, and extract only files from the archive absent from the base commit. Compare the resulting modified and added file set, SHA-256 hashes, and executable bits with the manifest. The final overlay adds the deployment configuration and edits the build, packaging and resource-staging scripts, their documentation and the resource-staging test.

## Local artifacts

- [Asset-free public IPA](../dist/HorizonIOS-0.1.0-18840db9146b-4ae1ce693ad4-public-20fe206d0265/HorizonIOS.ipa) and [SHA-256](../dist/HorizonIOS-0.1.0-18840db9146b-4ae1ce693ad4-public-20fe206d0265/SHA256SUMS)
- [Personal IPA with a separate HorizonResources folder](../dist/HorizonIOS-0.1.0-18840db9146b-4ae1ce693ad4-personal-aca4b22c21e4/HorizonIOS.ipa)
- [Private bundled IPA](../dist/HorizonIOS-0.1.0-18840db9146b-4ae1ce693ad4-personal-692f0e549373/HorizonIOS.ipa) and [SHA-256](../dist/HorizonIOS-0.1.0-18840db9146b-4ae1ce693ad4-personal-692f0e549373/SHA256SUMS)
- [iOS 16 reconstruction IPA](../.build/baseline-ios16.ipa). SHA-256 `964206c19199a28c111e5c3b4cfd3ede245a913acc732bbf8875684141c6f09f`.
- [Latest upstream compatibility report](../.build/latest-final.json)
