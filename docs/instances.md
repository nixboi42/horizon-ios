# Instances

Use the launcher menu beside the instance selector to create, rename, duplicate, or delete an instance. Duplicating copies worlds, settings, and packs. Deletion asks for confirmation. The offline username in launcher Settings applies to all instances.

**Play** opens the game title screen. **Save and Quit to Title** and **Disconnect** return there. **Quit Game** returns to the launcher so you can switch instances.

## Storage

Under `Library/Application Support/HorizonIOSDiagnostic/` in the app container:

- `Instances/catalog.json` stores names and selection by UUID.
- `Instances/<uuid>/` contains the instance’s `options.txt`, settings, `saves/`, and `resourcepacks/`. Datapacks live under `saves/<world>/datapacks/`.
- `resources/` holds verified payloads by index digest and an `active.json` pointer. All instances use the same active import.

On first launch, supported data from the legacy `Gameplay/` folder is copied into the default instance. The original stays available for recovery. Migration stops if it encounters a symbolic link in legacy save data. Duplicating or migrating worlds requires free space for the copy.
