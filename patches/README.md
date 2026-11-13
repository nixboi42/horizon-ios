# Patch order

The first five patches and the original overlay reconstruct the iOS working source at the base named in `docs/baseline-source.json`. Each patch uses full Git blob IDs for three-way application.

1. **Workspace and platform dependencies** wires the iOS crate into Cargo and selects compatible clipboard, threading, memory and HDR implementations.
2. **Metal fallbacks and safe-area layout** carries capability-dependent world rendering and shared hotbar geometry.
3. **Lifecycle, input and native keyboard** connects the gameplay and render APIs to the host overlay. It covers integrated-server startup, remote controls, menus, editing and hotbar routing.
4. **Network diagnostics** preserves packet context and the existing troubleshooting path.
5. **Release and contribution guidance** captures the prior edits to upstream docs and ignore rules.
6. **Crash-report paths** uses the same virtual iOS workspace prefix as the Rust build flags, so the executable omits the build machine's home path.

When upstream changes an integration interface, update the smallest affected patch and its overlay callers together.

The original series above remains available for the original lock. The candidate
series in `next/` separates workspace membership, platform adapters, and Metal
configuration. It generates the downstream Cargo.lock from upstream's pins
after applying the source patches and overlay. See [the rebase notes](../docs/upstream-resilience.md)
for the interfaces that moved and [maintenance](../README.md#maintain-the-port)
for the regeneration sequence.
