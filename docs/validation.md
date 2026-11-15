# Validation

| Build or environment | Recorded result |
| --- | --- |
| v0.2.0 build 11 unsigned IPA | Arm64, iOS 15.0 minimum, bundle metadata and privacy checks passed. The original archive checksum is in [BUILD.json](../BUILD.json). |
| v0.2.0 build 11 simulator | Survival gameplay captured on an iPhone 17 Pro simulator. |
| Earlier iOS 15-target build on iPhone 13 mini (iOS 27.2.1) | Installed, imported prepared resources, initialized Metal, and entered a local world. Earlier runs also covered touch, native text input and saved-position restore. |
| Earlier build on iPad 5 (A9, 2 GB, iPadOS 16.7.16) | Reached gameplay. This predates the current launcher and constrained rendering changes. |
| Earlier arm64 simulator builds | Covered integrated play, block break/place, saving and reopening, and native keyboard navigation. The multiplayer address paste test needed an [isolated test adjustment](../tests/simulator/README.md). |

Build 11 has no recorded physical-device acceptance run. iOS 15 is the deployment target. Long sessions, older-device startup, audio interruptions, and interrupted-save recovery remain open. See [known issues](known-issues.md).

## Multiplayer

Offline profiles were exercised with Paper 26.3 and Paper 26.1.2 with ViaVersion 5.12.2, including joins, chat, and chunk delivery. Simple Voice Chat 2.6.27 could coexist in the test setup. The affected GrimAC build reproduced the same server-side kick with vanilla clients. The fixed build also passed reconnect, player visibility, and block break/place checks. Voice transmission and online-mode login were outside those tests.

Development builds can set `HORIZON_PACKET_DIAGNOSTICS=1` to distinguish a server-issued disconnect from a local decode error. The bounded trace records packet metadata and redacts server text.

## Reproduce source checks

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests
./build.sh --locked --check
./build.sh --candidate --check
```

Use Python 3.11+. For build and packaging checks, follow the [build guide](building.md). For original source provenance, see the [baseline manifest](baseline-source.json). The original port and release snapshots remain in the repository.
