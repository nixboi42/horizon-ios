# v0.2.0 instance management

Select an instance in the launcher before pressing **Play**. The adjacent menu creates,
renames, duplicates, and deletes instances. Duplicating copies the selected instance's
worlds, settings, and packs and selects the copy. Deletion asks for confirmation.
The offline username in launcher Settings applies across instances.

Play opens Minecraft's main menu for the selected instance. Use **Singleplayer** to choose or
create a world, or **Multiplayer** to join a server. **Save and Quit to Title** and **Disconnect**
return to the Minecraft main menu. **Quit Game** there returns to the SwiftUI launcher, where
you can press Play again or switch instances.

Choose **Import Prepared Resources** and select the folder produced by
`ios/scripts/pack-resources.py`. The launcher displays the pinned Minecraft version
and whether the shared import is missing, incomplete, incompatible, or verified.
The existing bundle and Documents sources are used when no picker import is active.
If you replace the active resources after playing, close and reopen the app to activate the
new copy. The engine's data-fixer tables retain their first resource root for the process.

## App container

- `Library/Application Support/HorizonIOSDiagnostic/Instances/catalog.json` holds the
  selection and instance names keyed by stable UUIDs. Writes synchronize a temporary file
  before atomically renaming it into place.
- `Instances/<uuid>/` is the game directory for one session. It holds `options.txt`,
  Horizon settings files, `saves/`, `resourcepacks/`, and `saves/<world>/datapacks/`.
  The Rust session uses Horizon's existing pack discovery and save formats.
- `resources/<SHA-256 of mobile-import.json>/` holds a shared verified resource payload.
  `resources/active.json` points to the last successful picker import. Instances share
  those files. Horizon can generate disposable startup caches alongside the verified files.
- The first run stages a copy of the supported legacy data from `Gameplay/` before publishing
  the default instance and catalog in one rename. The original stays available for rollback.
  Later runs use the catalog, even after the last instance is deleted.
- Test launches with `HORIZON_IOS_TEST_SESSION` keep their own catalog under `Tests/<session>/`.
  `HORIZON_IOS_CONTROL=1` exposes `ctl.sock` in the selected test instance.

Deletion only targets the instance's UUID directory and never follows its pack-cache links.
A duplicate or migration can require space for a second copy of the worlds. A killed process
between a catalog update and a directory removal can leave an unlisted directory for recovery.
Earlier shared resource versions also remain on disk. If a legacy save contains a symbolic
link, migration stops and leaves the original in place for manual recovery.

## Validation after compilation is authorized

Run from the downstream checkout with a Python 3.11+ executable in `PYTHON`:

```sh
PYTHON=python3.14 ./build.sh --candidate --compile-only --test
```

For the simulator tests, set `SOURCE` to the candidate tree under `.build/source/`,
`SIMULATOR` to a disposable arm64 simulator UDID, and `PREPARED` to the absolute path
to an owned `HorizonResources` folder. Use a disposable simulator with no personal app data.
Build the tests and install their app before staging resources:

```sh
cd "$SOURCE"
export HORIZON_CARGO="$(command -v cargo)"
xcodebuild -project ios/HorizonIOS.xcodeproj -scheme HorizonIOS \
  -configuration Release -destination "id=$SIMULATOR" -derivedDataPath .local/phase2-ui \
  CODE_SIGNING_ALLOWED=NO build-for-testing
xcrun simctl install "$SIMULATOR" .local/phase2-ui/Build/Products/Release-iphonesimulator/HorizonIOS.app
APP_DATA=$(xcrun simctl get_app_container "$SIMULATOR" dev.horizon.ios data)
mkdir -p "$APP_DATA/Documents"
ditto "$PREPARED" "$APP_DATA/Documents/HorizonResources"
xcodebuild -project ios/HorizonIOS.xcodeproj -scheme HorizonIOS \
  -configuration Release -destination "id=$SIMULATOR" -derivedDataPath .local/phase2-ui \
  -only-testing:HorizonIOSUITests/NavigationKeyboardTests \
  CODE_SIGNING_ALLOWED=NO test-without-building
```

On iPhone and iPad, verify Files import from a local folder and from iCloud Drive, including
cancellation and an incompatible folder. Upgrade over a disposable Phase 1 installation with
a saved world and changed settings. Open the migrated world, change a setting, switch instances,
and check that the other instance's settings and worlds are independent. Check that the legacy
world is still present, then verify relaunch, duplicate, delete, camera drag, offline username,
and save-on-return on the oldest available OS.
