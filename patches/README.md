# Patch layout

The builder applies patches in `series` order. Patches carry full Git blob IDs for three-way application.

| Location | Purpose |
| --- | --- |
| `patches/series` and `overlay/` | Original port and overlay, available through `--baseline` |
| `patches/next/` | Working series, pinned by `base`. Its overlay replaces matching files in the shared overlay |
| `patches/verified-*/` | Frozen patch and overlay snapshots selected by release locks |

Keep frozen snapshots intact. The builder fingerprints patch content, overlay content and executable bits, and the patch base. The build 11 release digest is recorded in [BUILD.json](../BUILD.json). Original source provenance is in the [baseline manifest](../docs/baseline-source.json).

## Maintain the patch stack

Update the patch and any overlay callers together when an upstream interface changes. Check `./build.sh --candidate --check` at the pinned base, then `./build.sh --latest --check` against upstream HEAD. Reports go under `.build/` and identify any patch or source-interface failure. The builder reconciles the Cargo lock against upstream’s dependency pins and rejects external package identity changes.

The rebase to `e9358e2892e1a348aca5e96fa9308458d7566e5d` split workspace, platform adapter, and Metal changes into separate patches and adapted the loader, renderer, and protocol interfaces. The original series remains available for comparison.

For promotion, `--update --expect-upstream SHA` requires matching prepared resources or the official server JAR. It builds and tests both package modes before freezing the verified inputs and updating `upstream.lock`. See [build options](../docs/building.md#builder-modes).
