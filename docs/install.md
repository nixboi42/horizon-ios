# Install Horizon iOS

You need an arm64 iPhone or iPad running iOS or iPadOS 15 or later, and access to a Mac to prepare resources from your own Minecraft: Java Edition 26.3 installation. See the [device validation record](validation.md) for tested hardware.

1. Download [HorizonIOS-0.2.0-11-touch-toggles-unsigned.ipa](https://github.com/nixboi42/horizon-ios/releases/download/v0.2.0-beta.11/HorizonIOS-0.2.0-11-touch-toggles-unsigned.ipa) and [SHA256SUMS](https://github.com/nixboi42/horizon-ios/releases/download/v0.2.0-beta.11/SHA256SUMS) from the [build 11 release](https://github.com/nixboi42/horizon-ios/releases/tag/v0.2.0-beta.11). In the download directory, run `shasum -a 256 -c SHA256SUMS`.
2. Re-sign and install the IPA with a sideloading tool and your Apple ID or development team. Follow the tool’s instructions for device trust, developer settings, and signature renewal.
3. [Prepare the game resources](building.md#prepare-game-resources). Transfer the resulting `HorizonResources` folder to a location accessible in Files. Keep `mobile-import.json` and the rest of the folder together.
4. Open Horizon iOS, tap **Import Prepared Resources**, and select the folder. Wait for verification to finish. If you replace the import after playing, restart the app before using the new copy.
5. Select an instance and tap **Play**. At the game title screen, choose **Singleplayer** to create or open a world, or **Multiplayer → Direct Connection** to join a compatible offline-mode Java server.

**Save and Quit to Title** returns you to the game menu. **Quit Game** there returns you to the launcher. See [instances](instances.md) and [touch controls](touch-controls.md).

## Updates and troubleshooting

- Back up worlds before updating. Install over the existing app with the same bundle identifier and signing configuration to retain the app container. Deleting the app removes its worlds, settings, and imported resources.
- For a failed import, prepare again for 26.3 and transfer the entire folder. For missing audio, import a complete resource folder prepared with sounds.
- For provisioning errors or an expired signature, use your signing tool to check device eligibility and re-sign the IPA.
- See [known issues](known-issues.md) for multiplayer, device, and input limitations.
