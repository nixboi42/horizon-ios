//! Private game directories and the launcher's atomic instance catalog.
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const DEFAULT_ID: &str = "00000000-0000-0000-0000-000000000001";
const PLAYER_DATA: &[&str] = &[
    "saves",
    "resourcepacks",
    "shaderpacks",
    "backups",
    "options.txt",
    "horizon-graphics.txt",
    "horizon-controller.txt",
    "horizon-extras.txt",
    "horizon-mods.txt",
    "horizon-player.txt",
    "servers.dat",
    "servers.dat_old",
    "hotbar.nbt",
    "screenshots",
];

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn instance_id(id: &str) -> io::Result<&str> {
    if id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_digit() || matches!(b, b'a'..=b'f'),
        })
    {
        Ok(id)
    } else {
        Err(invalid("invalid instance ID"))
    }
}

fn display_name(name: Option<&str>) -> io::Result<&str> {
    name.map(str::trim)
        .filter(|s| !s.is_empty() && s.chars().count() <= 64 && !s.chars().any(char::is_control))
        .ok_or_else(|| invalid("invalid instance name"))
}

fn directory(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        Ok(())
    } else {
        Err(invalid("expected a private directory"))
    }
}

fn read_json(path: &Path) -> io::Result<Value> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(invalid("expected a regular catalog file"));
    }
    Ok(serde_json::from_reader(fs::File::open(path)?)?)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("incoming");
    // create_new won't follow a symlink. A previous interrupted write can be unlinked.
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let result = (|| {
        let mut file = fs::File::create_new(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

// The same walk validates game data on Play and copies it for migration/duplication.
// Never follow a symlink or copy a device, socket or FIFO.
fn game_data(source: &Path, target: Option<&Path>) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_dir() {
        if let Some(destination) = target {
            fs::create_dir(destination)?;
        }
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let destination = target.map(|p| p.join(entry.file_name()));
            game_data(&entry.path(), destination.as_deref())?;
        }
    } else if metadata.is_file() {
        if let Some(destination) = target {
            fs::copy(source, destination)?;
        }
    } else {
        return Err(invalid(
            "links and special files are unsupported in game data",
        ));
    }
    Ok(())
}

fn setup_game(destination: &Path, source: Option<&Path>) -> io::Result<()> {
    fs::create_dir(destination)?;
    if let Some(source) = source {
        directory(source)?;
        for name in PLAYER_DATA {
            let from = source.join(name);
            match fs::symlink_metadata(&from) {
                Ok(_) => game_data(&from, Some(&destination.join(name)))?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
    }
    for name in ["saves", "resourcepacks"] {
        let path = destination.join(name);
        if !path.exists() {
            fs::create_dir(&path)?;
        }
        directory(&path)?;
    }
    Ok(())
}

pub(crate) struct Store {
    pub(crate) parent: PathBuf,
    pub(crate) legacy: PathBuf,
}

impl Store {
    fn root(&self) -> PathBuf {
        self.parent.join("Instances")
    }

    fn initialize(&self) -> io::Result<()> {
        directory(&self.parent)?;
        let root = self.root();
        match fs::symlink_metadata(&root) {
            Ok(_) => return directory(&root),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let legacy = match fs::symlink_metadata(&self.legacy) {
            Ok(_) => Some(self.legacy.as_path()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let staging = self.parent.join(".instances-incoming");
        staged(&staging, |path| {
            fs::create_dir(path)?;
            setup_game(&path.join(DEFAULT_ID), legacy)?;
            let catalog = json!({
                "schema": 1, "selected": DEFAULT_ID,
                "instances": [{"id": DEFAULT_ID, "name": null}]
            });
            atomic_write(&path.join("catalog.json"), &serde_json::to_vec(&catalog)?)?;
            fs::rename(path, root)
        })
    }

    fn catalog(&self) -> io::Result<Value> {
        self.initialize()?;
        let catalog = read_json(&self.root().join("catalog.json"))?;
        if catalog["schema"] != 1 {
            return Err(invalid("unsupported instance catalog schema"));
        }
        let rows = catalog["instances"]
            .as_array()
            .ok_or_else(|| invalid("invalid catalog"))?;
        let mut ids = std::collections::HashSet::new();
        for row in rows {
            let id = instance_id(row["id"].as_str().unwrap_or_default())?;
            if !ids.insert(id) {
                return Err(invalid("duplicate instance ID"));
            }
            if !row["name"].is_null() {
                display_name(row["name"].as_str())?;
            }
            directory(&self.root().join(id))?;
        }
        let selected = catalog["selected"].as_str();
        if !(rows.is_empty() && catalog["selected"].is_null()
            || selected.is_some_and(|id| ids.contains(id)))
        {
            return Err(invalid("invalid selected instance"));
        }
        Ok(catalog)
    }

    fn commit(&self, catalog: &Value) -> io::Result<()> {
        atomic_write(
            &self.root().join("catalog.json"),
            &serde_json::to_vec(catalog)?,
        )
    }

    pub(crate) fn request(&self, request: &Value) -> io::Result<Value> {
        let mut catalog = self.catalog()?;
        let operation = request["operation"].as_str().unwrap_or_default();
        if operation == "list" {
            return Ok(catalog);
        }
        let id = instance_id(request["id"].as_str().unwrap_or_default())?;
        let name = request["name"].as_str();
        let rows = catalog["instances"].as_array_mut().unwrap();
        let row = rows.iter().position(|row| row["id"] == id);
        match (operation, row) {
            ("create" | "duplicate", None) => {
                let name = display_name(name)?;
                let source = if operation == "duplicate" {
                    let source = instance_id(request["source"].as_str().unwrap_or_default())?;
                    if !rows.iter().any(|row| row["id"] == source) {
                        return Err(invalid("unknown source instance"));
                    }
                    Some(self.root().join(source))
                } else {
                    None
                };
                let destination = self.root().join(id);
                match fs::symlink_metadata(&destination) {
                    Ok(_) => return Err(invalid("instance directory already exists")),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
                let staging = self.root().join(".game-incoming");
                staged(&staging, |path| {
                    setup_game(path, source.as_deref())?;
                    fs::rename(path, &destination)
                })?;
                rows.push(json!({"id": id, "name": name}));
                catalog["selected"] = json!(id);
                if let Err(error) = self.commit(&catalog) {
                    let _ = fs::remove_dir_all(destination);
                    return Err(error);
                }
                return Ok(catalog);
            }
            ("select", Some(_)) => catalog["selected"] = json!(id),
            ("rename", Some(row)) => rows[row]["name"] = json!(display_name(name)?),
            ("delete", Some(row)) => {
                rows.remove(row);
                let replacement = rows
                    .get(row)
                    .or_else(|| rows.last())
                    .map(|row| row["id"].clone());
                if catalog["selected"] == id {
                    catalog["selected"] = replacement.unwrap_or(Value::Null);
                }
                self.commit(&catalog)?;
                // remove_dir_all unlinks directory symlinks without traversing them.
                if let Err(error) = fs::remove_dir_all(self.root().join(id)) {
                    log::warn!("Could not remove deleted instance directory: {error}");
                }
                return Ok(catalog);
            }
            _ => return Err(invalid("unknown instance or operation")),
        }
        self.commit(&catalog)?;
        Ok(catalog)
    }

    pub(crate) fn game_dir(&self, id: &str) -> io::Result<PathBuf> {
        let id = instance_id(id)?;
        if self.catalog()?["selected"] != id {
            return Err(invalid("instance is not selected"));
        }
        let dir = self.root().join(id);
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            match entry.file_name().to_str() {
                // Pack overlays link their contents to vanilla files. Their own
                // directories need to stay inside the instance.
                Some(".horizon-cache") => {
                    let cache = entry.path();
                    directory(&cache)?;
                    let packs = cache.join("resourcepacks");
                    match fs::symlink_metadata(&packs) {
                        Ok(meta) if meta.is_dir() => {
                            for overlay in fs::read_dir(packs)? {
                                let overlay = overlay?;
                                if overlay.file_type()?.is_symlink() {
                                    return Err(invalid("linked pack cache root"));
                                }
                            }
                        }
                        Ok(_) => return Err(invalid("invalid pack cache root")),
                        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e),
                    }
                }
                _ => game_data(&entry.path(), None)?,
            }
        }
        directory(&dir.join("saves"))?;
        directory(&dir.join("resourcepacks"))?;
        Ok(dir)
    }
}

fn staged(path: &Path, publish: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let result = publish(path);
    if result.is_err() {
        let _ = fs::remove_dir_all(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "11111111-1111-4111-8111-111111111111";
    const SECOND: &str = "22222222-2222-4222-8222-222222222222";

    struct Fixture(Store);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let parent = std::env::temp_dir().join(format!(
                "horizon-instances-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&parent).unwrap();
            Self(Store {
                legacy: parent.join("Gameplay"),
                parent,
            })
        }

        fn apply(&self, request: Value) -> Value {
            self.0.request(&request).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0.parent);
        }
    }

    #[test]
    fn upgrades_legacy_data_once_even_after_default_is_deleted() {
        let fixture = Fixture::new();
        let legacy = &fixture.0.legacy;
        let pack = Path::new("saves/My World/datapacks/pack.zip");
        fs::create_dir_all(legacy.join(pack.parent().unwrap())).unwrap();
        fs::write(legacy.join(pack), b"datapack").unwrap();
        fs::create_dir(legacy.join("resourcepacks")).unwrap();
        fs::write(legacy.join("resourcepacks/pack.zip"), b"resource pack").unwrap();
        fs::write(legacy.join("options.txt"), b"legacy options").unwrap();
        fs::write(legacy.join("horizon-extras.txt"), b"legacy extras").unwrap();
        // Simulate an interrupted attempt before publication.
        fs::create_dir_all(fixture.0.parent.join(".instances-incoming/unfinished")).unwrap();

        let first = fixture.apply(json!({"operation": "list"}));
        assert_eq!(first["selected"], DEFAULT_ID);
        let dir = fixture.0.game_dir(DEFAULT_ID).unwrap();
        assert_eq!(fs::read(dir.join(pack)).unwrap(), b"datapack");
        assert_eq!(
            fs::read(dir.join("resourcepacks/pack.zip")).unwrap(),
            b"resource pack"
        );
        assert_eq!(
            fs::read(dir.join("horizon-extras.txt")).unwrap(),
            b"legacy extras"
        );
        fs::write(dir.join("options.txt"), b"new settings").unwrap();
        assert_eq!(fixture.apply(json!({"operation": "list"})), first);
        assert_eq!(fs::read(dir.join("options.txt")).unwrap(), b"new settings");
        assert_eq!(
            fs::read(legacy.join("options.txt")).unwrap(),
            b"legacy options"
        );

        fixture.apply(json!({"operation": "delete", "id": DEFAULT_ID}));
        assert!(fixture.apply(json!({"operation": "list"}))["selected"].is_null());
        assert!(!dir.exists());
        assert_eq!(fs::read(legacy.join(pack)).unwrap(), b"datapack");
    }

    #[test]
    fn selects_and_duplicates_independent_worlds_and_settings() {
        let fixture = Fixture::new();
        fixture.apply(json!({"operation": "create", "id": FIRST, "name": "../Display Name"}));
        let original = fixture.0.game_dir(FIRST).unwrap();
        assert_eq!(original.file_name().unwrap(), FIRST);
        fs::write(original.join("options.txt"), b"renderDistance:4").unwrap();
        fs::create_dir(original.join("saves/World")).unwrap();
        fs::write(original.join("saves/World/level.dat"), b"world").unwrap();
        fixture.apply(
            json!({"operation": "duplicate", "id": SECOND, "name": "Copy", "source": FIRST}),
        );
        let copy = fixture.0.game_dir(SECOND).unwrap();
        assert!(fixture.0.game_dir(FIRST).is_err());
        assert_eq!(
            fs::read(copy.join("options.txt")).unwrap(),
            b"renderDistance:4"
        );
        assert_eq!(
            fs::read(copy.join("saves/World/level.dat")).unwrap(),
            b"world"
        );
        fs::write(copy.join("options.txt"), b"renderDistance:8").unwrap();
        assert_eq!(
            fs::read(original.join("options.txt")).unwrap(),
            b"renderDistance:4"
        );
        fixture.apply(json!({"operation": "rename", "id": FIRST, "name": "Renamed"}));
        fixture.apply(json!({"operation": "select", "id": FIRST}));
        assert_eq!(fixture.0.game_dir(FIRST).unwrap(), original);
        let catalog = fixture.apply(json!({"operation": "delete", "id": SECOND}));
        assert_eq!(catalog["selected"], FIRST);
        assert!(!copy.exists());
        assert!(original.join("saves/World/level.dat").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_linked_data_and_path_ids_and_preserves_shared_assets() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let shared = fixture.0.parent.join("resources");
        fs::create_dir(&shared).unwrap();
        fs::write(shared.join("asset"), b"shared").unwrap();
        fs::create_dir_all(fixture.0.legacy.join("saves")).unwrap();
        let link = fixture.0.legacy.join("saves/unsafe");
        symlink(&shared, &link).unwrap();
        assert!(fixture.0.request(&json!({"operation": "list"})).is_err());
        assert!(!fixture.0.root().exists());
        fs::remove_file(link).unwrap();

        fixture.apply(json!({"operation": "create", "id": FIRST, "name": "Test"}));
        assert!(
            fixture
                .0
                .request(&json!({"operation": "delete", "id": "../resources"}))
                .is_err()
        );
        let game = fixture.0.game_dir(FIRST).unwrap();
        symlink(&shared, game.join("saves/unsafe")).unwrap();
        assert!(fixture.0.game_dir(FIRST).is_err());
        assert!(
            fixture
                .0
                .request(&json!({
                    "operation": "duplicate", "id": SECOND, "source": FIRST, "name": "Copy"
                }))
                .is_err()
        );
        fixture.apply(json!({"operation": "delete", "id": FIRST}));
        assert_eq!(fs::read(shared.join("asset")).unwrap(), b"shared");
    }
}
