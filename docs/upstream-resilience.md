# Downstream rebase to e9358e2892e1a

The official default branch was `main` at `e9358e2892e1a348aca5e96fa9308458d7566e5d` when resolved on November 13, 2026. The original stack failed at the same four paths on that commit and on the earlier `d3bd732245543d66f4a1cd672862700f38844fb8`. The six original patches and the working overlay remain in place. The pre-change archive and checksums are under `.build/preservation/`.

| Original patch 01 conflict | Cause and resolution |
| --- | --- |
| `Cargo.lock` | Upstream expanded the workspace and changed edges in the same package records. The input crate lost DualSense and the threads crate gained unwinding dependencies, while additional workspace packages changed the insertion neighborhood. The new series starts from the selected upstream lock, runs Cargo metadata with the edited manifests, audits package identities, then validates with locked metadata. |
| `crates/horizon-client-input/Cargo.toml` | Upstream moved the window icon dependency to dev-dependencies and moved DualSense out of the input crate. Upstream still declares arboard unconditionally. The iOS series preserves upstream's layout and features, gates arboard to other platforms and adds the iOS-only clipboard adapter link. |
| `crates/horizon-hdr/src/lib.rs` | Upstream removed the paper-white argument, split metadata generation into a helper, and added ray-query feature opt-in. Vulkan still used a macOS-only exclusion. The source and ash dependency now use Apple's target vendor as the boundary, keeping upstream's metadata calculation and tests on Vulkan targets. |
| `crates/horizon-threads/src/lib.rs` | Upstream expanded the module list for allocation, sampling, unwind and work scheduling. The clipboard module remains required and has an independent iOS-gated declaration. Upstream already ships a Mach-task resident-memory implementation, so the candidate drops the duplicate and retains upstream's implementation. The iOS check also exposed a signedness mismatch at the new mimalloc JSON C string boundary, fixed by casting to the target's C character type. |

The candidate splits workspace membership, platform adapters and Metal dependencies into three patches. Patch 02 preserves upstream's DirectX adapter selection and puts its Apple exclusion at the relocated surface setup. The upstream TAA regression and downstream hotbar regression both remain. Patch 03 uses the common settings shell for native search editing, reapplies startup handoff to the extracted loading modules, keeps server timing within upstream's panic recovery, and uses the layered HUD for safe-area offsets. The iOS overlay callers adopt the new replay, startup, environment and renderer APIs. Patch 04 preserves the new protocol adapter APIs and upstream recording test alongside diagnostics. Patch 05 now carries only ignore rules. Port documentation remains in the downstream tree. Patch 06 applied cleanly. Release inspection then found a second build-machine path in the upstream data-fixer fallback. iOS now registers the prepared resource root at startup and uses a sandbox-relative fallback.

The generated Cargo lock adds the local `horizon-ios` workspace member, the input crate's link to `horizon-threads`, and the threads crate's links to upstream-pinned `objc2-foundation 0.2.2` and `objc2-ui-kit 0.2.2`. Every existing package name, version, source and checksum was retained. The builder rejects external package identity changes pending an explicit review. There is no workspace-wide `cargo update`.

See the [maintenance workflow](../README.md#maintain-the-port) for the staged patch regeneration and promotion commands.

## Validation and promotion

- `--latest --check` reconstructed tree `4e98763c76d15a19a894af88cedfc5a2694f9429` and passed locked Cargo metadata.
- Host compilation, arm64 iOS checks and Xcode Release linking passed. Both IPA executables and plists report iOS 15.0 and support the iPhone and iPad device families.
- Seven downstream builder tests, 11 iOS Python tests, 15 iOS host unit tests, five focused mobile input, hotbar and native editing tests, and both resource-backed integrated and remote session tests passed. The separate terrain unit test remained ignored.
- Fresh Minecraft 26.3 preparation passed `horizon-prepare --check`. The bundled IPA's index matched all 28,182 staged files. The public IPA passed archive and binary privacy checks.
- Both isolated simulator keyboard cases passed with the original locked game library. See the [simulator diagnostic](../tests/simulator/README.md). The rebased application has no physical-device test in this run, including on iOS 15 or iPad 5.
- `--update --expect-upstream e9358e2892e1a348aca5e96fa9308458d7566e5d` promoted the verified source and overlay after checking both packages. The original revision remains available via `--baseline`.

The [compatibility report](../.build/resilience-final-update.json) records both release paths and checksums. The [original conflict reproduction](../.build/resilience-conflict.json) records the four paths and their upstream diffs. That failed checkout produced no reconstruction receipt or package. The pre-existing IPAs and original source snapshot still match their recorded checksums.
