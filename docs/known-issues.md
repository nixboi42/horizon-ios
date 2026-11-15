# Known issues in v0.2.0 build 11

- **Accounts and multiplayer.** Use an offline profile and a compatible Java Edition server in offline mode. Microsoft sign-in and online-mode servers are planned. Controller support is unavailable.
- **Older server plugins.** GrimAC 2.3.74-ba1a930 can kick Java 26.3 clients with “Invalid packet” because its bundled PacketEvents lacks the Waypoint packet. Vanilla clients reproduced the kick. The test passed after replacing GrimAC with 2.3.75-85d931f. Updating a separate PacketEvents JAR leaves GrimAC’s bundled copy in place.
- **Older devices.** The arm64 binary targets iOS 15.0. Build 11 has no recorded physical acceptance run on iOS 15 or the 2 GB iPad 5. Earlier iOS 15 reports included a Metal startup abort. The constrained rendering path lowers memory pressure. Its effectiveness against that abort still needs a physical test.
- **Audio.** Import resources with sounds. Bluetooth route changes, interruptions, and long playback need device validation.
- **Saves.** Use **Save and Quit to Title** before force-quitting. Background saves have a limited time window. Interrupted-save recovery and modified-block persistence after relaunch need more physical-device coverage.
- **Resource replacement.** Restart the app if you import a different resource set after playing.

For a bug report, include the app version and build, device model and OS, reproduction steps, and a redacted console log. Include server and plugin versions for multiplayer issues. Keep private data and game files out of attachments. See [validation](validation.md).
