//! Prepared resource import. This verifies transfer integrity, not ownership or
//! authenticity. Assets must come from the player's own prepared installation.
//!
//! The native picker installs an active shared import. Bundle and Documents
//! sources also work for developer builds and existing installations.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn incomplete(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::NotFound {
        invalid("prepared resources are missing an indexed file")
    } else {
        error
    }
}
fn relative(name: &str) -> io::Result<&Path> {
    let path = Path::new(name);
    if name.split('/').any(|part| matches!(part, "" | "." | ".."))
        || name.contains('\\')
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid("resource index contains an unsafe path"));
    }
    Ok(path)
}
fn digest(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn verify(root: &Path, files: &serde_json::Map<String, Value>) -> io::Result<()> {
    verify_tree(root, files, true).map(|_| ())
}

// A receipt only accelerates reads of the private, content-addressed store.
// External imports hash every byte in staging before publication. Warm reads
// also check paths: symlinks, truncation and same-size edits invalidate receipts.
fn verify_tree(
    root: &Path,
    files: &serde_json::Map<String, Value>,
    hash_payload: bool,
) -> io::Result<String> {
    let mut stamp = Sha256::new();
    // The signed bundle / private staging tree is stable during a pass. Check
    // shared parent directories once instead of stat-ing them for every one
    // of tens of thousands of leaves. Full passes also hash every file.
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(invalid("expected a resource directory"));
    }
    let mut directories = std::collections::HashSet::new();
    for (name, expected) in files {
        let path = relative(name)?;
        let mut current = root.to_path_buf();
        let mut components = path.components().peekable();
        while let Some(part) = components.next() {
            current.push(part);
            let parent = components.peek().is_some();
            if parent && directories.contains(&current) {
                continue;
            }
            let metadata = fs::symlink_metadata(&current)?;
            if metadata.file_type().is_symlink() {
                return Err(invalid("resource symlinks are not supported"));
            }
            if parent {
                if !metadata.is_dir() {
                    return Err(invalid("resource parent is not a directory"));
                }
                directories.insert(current.clone());
            } else {
                if !metadata.is_file() {
                    return Err(invalid("resource is not a regular file"));
                }
                stamp.update((name.len() as u64).to_le_bytes());
                stamp.update(name.as_bytes());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    for value in [
                        metadata.dev(),
                        metadata.ino(),
                        metadata.len(),
                        metadata.mtime() as u64,
                        metadata.mtime_nsec() as u64,
                        metadata.ctime() as u64,
                        metadata.ctime_nsec() as u64,
                    ] {
                        stamp.update(value.to_le_bytes());
                    }
                }
            }
        }
        if hash_payload && Some(digest(&current)?.as_str()) != expected.as_str() {
            return Err(invalid("resource SHA-256 mismatch"));
        }
    }
    let manifest: Value = serde_json::from_slice(&fs::read(root.join("horizon-resources.json"))?)?;
    if manifest["data_version"].as_i64() != Some(horizon_core::version::DATA_VERSION.into())
        || manifest["schema"].as_i64() != Some(7)
        || manifest["minecraft"].as_str() != Some(horizon_core::VANILLA_VERSION)
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "prepared resources do not match Horizon data version/schema",
        ));
    }
    Ok(format!("{:x}", stamp.finalize()))
}
/// Folder name expected inside the app's Documents directory.
pub const SHARED_FOLDER: &str = "HorizonResources";

/// Locate the first source that carries a mobile import index.
///
/// A developer build embeds `PreparedResources` in the app bundle. A signed but
/// asset-free build instead reads `Documents/HorizonResources`, copied there by
/// the player with Finder or the Files app.
pub fn source(bundle: &Path, documents: &Path) -> io::Result<PathBuf> {
    let bundled = bundle.join("PreparedResources");
    if bundled.join("mobile-import.json").is_file() {
        return Ok(bundled);
    }
    let shared = documents.join(SHARED_FOLDER);
    if shared.join("mobile-import.json").is_file() {
        return Ok(shared);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Prepared resources are missing. Copy the HorizonResources folder made by \
         ios/scripts/pack-resources.py into the app's Documents folder (Finder or the \
         Files app), then relaunch.",
    ))
}

/// Import resources from whichever source is present.
#[cfg(test)]
fn import_from(bundle: &Path, documents: &Path, storage: &Path) -> io::Result<PathBuf> {
    let source = source(bundle, documents)?;
    eprintln!("RESOURCE_IMPORT source={}", source.display());
    import(&source, storage)
}

const INDEX: &str = "mobile-import.json";
const ACTIVE: &str = "active.json";

fn read_index(root: &Path) -> io::Result<Vec<u8>> {
    let index = root.join(INDEX);
    if !fs::symlink_metadata(root)?.is_dir() || !fs::symlink_metadata(&index)?.is_file() {
        return Err(invalid(
            "expected a resource folder and regular import index",
        ));
    }
    fs::read(index)
}

fn file_index(index: &Value) -> io::Result<&serde_json::Map<String, Value>> {
    if index["format"].as_u64() != Some(1) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported mobile import format",
        ));
    }
    let files = index["files"]
        .as_object()
        .ok_or_else(|| invalid("missing resource file index"))?;
    if !files.contains_key("horizon-resources.json")
        || files.len() > 200_000
        || files.keys().any(|name| {
            let leaf = name.split('/').next().unwrap_or_default();
            [
                INDEX,
                "mobile-import.incoming",
                "horizon-data.bin",
                "horizon-sprites.cache",
            ]
            .iter()
            .any(|reserved| leaf.eq_ignore_ascii_case(reserved))
        })
    {
        return Err(invalid("invalid resource file index"));
    }
    for name in files.keys() {
        relative(name)?;
    }
    Ok(files)
}

fn verify_for_play(root: &Path, files: &serde_json::Map<String, Value>) -> io::Result<()> {
    // pack-resources.py checks the full preparation before exporting it. Check
    // the core layout here too, so a manifest-only transfer can't enable Play.
    for name in [
        "facts.json",
        "block-behaviours.json",
        "block-tags.json",
        "gameplay-facts.json",
    ] {
        if !files.contains_key(name) || fs::metadata(root.join(name))?.len() == 0 {
            return Err(invalid("prepared resources are incomplete"));
        }
    }
    for prefix in ["assets/minecraft/", "data/minecraft/", "worldgen/"] {
        if !files.keys().any(|name| name.starts_with(prefix)) {
            return Err(invalid("prepared resources are incomplete"));
        }
    }
    Ok(())
}

fn verify_import(root: &Path) -> io::Result<Vec<u8>> {
    let bytes = read_index(root)?;
    let index = serde_json::from_slice(&bytes)?;
    let files = file_index(&index)?;
    // The active private copy was checked before publication. Warm launches
    // check the index and required files without walking the entire asset tree.
    verify_for_play(root, files)?;
    Ok(bytes)
}

// Receipts are disposable and versioned separately from prepared resources.
// ctime prevents preserving mtime from hiding an edit on APFS. This is a local
// integrity cache, not an authenticity boundary against a compromised sandbox.
fn receipt(bytes: &[u8], stamp: &str) -> String {
    format!(
        "v1:{}:{}:{:x}:{stamp}",
        horizon_core::VANILLA_VERSION,
        horizon_core::version::DATA_VERSION,
        Sha256::digest(bytes)
    )
}

fn save_receipt(root: &Path, bytes: &[u8], stamp: &str) {
    if let Err(error) = crate::instances::atomic_write(
        &root.with_extension("verified-v1"),
        receipt(bytes, stamp).as_bytes(),
    ) {
        eprintln!("RESOURCE_VERIFY receipt_write_failed={error}");
    }
}

fn verify_cached(
    root: &Path,
    files: &serde_json::Map<String, Value>,
    bytes: &[u8],
) -> io::Result<()> {
    let started = std::time::Instant::now();
    let before = verify_tree(root, files, false).map_err(incomplete)?;
    let hit = cfg!(unix)
        && fs::read_to_string(root.with_extension("verified-v1"))
            .is_ok_and(|cached| cached == receipt(bytes, &before));
    if !hit {
        verify(root, files).map_err(incomplete)?;
        let after = verify_tree(root, files, false).map_err(incomplete)?;
        if before != after {
            return Err(invalid("prepared resources changed during verification"));
        }
        save_receipt(root, bytes, &after);
    }
    eprintln!(
        "RESOURCE_VERIFY cache={} elapsed_ms={:.2} files={}",
        if hit { "hit" } else { "miss" },
        started.elapsed().as_secs_f64() * 1000.,
        files.len()
    );
    Ok(())
}

pub fn available(bundle: &Path, documents: &Path, storage: &Path) -> io::Result<PathBuf> {
    if fs::symlink_metadata(storage).is_ok_and(|meta| !meta.is_dir()) {
        return Err(invalid("expected a private resource store"));
    }
    let marker = storage.join(ACTIVE);
    match fs::symlink_metadata(&marker) {
        Ok(meta) if meta.is_file() && fs::symlink_metadata(storage)?.is_dir() => {
            let id: String = serde_json::from_slice(&fs::read(marker)?)?;
            if id.len() != 64
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
            {
                return Err(invalid("invalid shared resource ID"));
            }
            let root = storage.join(&id);
            let bytes = verify_import(&root).map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    invalid("active shared resources are incomplete")
                } else {
                    error
                }
            })?;
            if format!("{:x}", Sha256::digest(&bytes)) != id {
                return Err(invalid("shared resource index does not match its ID"));
            }
            Ok(root)
        }
        Ok(_) => Err(invalid("invalid active resource marker")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            install(&source(bundle, documents)?, storage)
        }
        Err(error) => Err(error),
    }
}

pub fn install(source: &Path, storage: &Path) -> io::Result<PathBuf> {
    let root = import_payload(source, storage, true)?;
    let id = root.file_name().and_then(|name| name.to_str()).unwrap();
    crate::instances::atomic_write(&storage.join(ACTIVE), &serde_json::to_vec(id)?)?;
    Ok(root)
}

#[cfg(test)]
fn import(source: &Path, storage: &Path) -> io::Result<PathBuf> {
    import_payload(source, storage, false)
}

fn import_payload(source: &Path, storage: &Path, playable: bool) -> io::Result<PathBuf> {
    let bytes = read_index(source).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            invalid("prepared resource folder has no import index")
        } else {
            error
        }
    })?;
    let index: Value = serde_json::from_slice(&bytes)?;
    let files = file_index(&index)?;
    let started = std::time::Instant::now();
    let id = format!("{:x}", Sha256::digest(&bytes));
    let destination = storage.join(id);
    match fs::symlink_metadata(storage) {
        Ok(meta) if !meta.is_dir() => return Err(invalid("expected a private resource store")),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if fs::symlink_metadata(&destination).is_ok() {
        verify_cached(&destination, files, &bytes)?;
        if playable {
            verify_for_play(&destination, files)?;
        }
        // Phase 1 cached the payload without the index.
        crate::instances::atomic_write(&destination.join(INDEX), &bytes)?;
        if !crate::TESTFLIGHT {
            eprintln!(
                "RESOURCE_IMPORT cache=hit verify_ms={:.2} files={}",
                started.elapsed().as_secs_f64() * 1000.,
                files.len()
            );
        }
        return Ok(destination);
    }
    // Validate layout before copying, then hash the private staging copy once.
    // A changed or corrupt external source can never pass publication below.
    verify_tree(source, files, false).map_err(incomplete)?;
    if playable {
        verify_for_play(source, files)?;
    }
    if !crate::TESTFLIGHT {
        eprintln!(
            "RESOURCE_IMPORT source_layout_ms={:.2} files={}",
            started.elapsed().as_secs_f64() * 1000.,
            files.len()
        );
    }
    fs::create_dir_all(storage)?;
    let staging = storage.join(".incoming");
    match fs::remove_dir_all(&staging) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::create_dir(&staging)?;
    let copying = std::time::Instant::now();
    let result = (|| {
        for name in files.keys() {
            let relative = relative(name)?;
            let target = staging.join(relative);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::copy(source.join(relative), target)?;
        }
        if !crate::TESTFLIGHT {
            eprintln!(
                "RESOURCE_IMPORT copy_ms={:.2}",
                copying.elapsed().as_secs_f64() * 1000.
            );
        }
        let verifying = std::time::Instant::now();
        verify(&staging, files)?;
        if playable {
            verify_for_play(&staging, files)?;
        }
        crate::instances::atomic_write(&staging.join(INDEX), &bytes)?;
        if !crate::TESTFLIGHT {
            eprintln!(
                "RESOURCE_IMPORT publication_verify_ms={:.2}",
                verifying.elapsed().as_secs_f64() * 1000.
            );
        }
        fs::rename(&staging, &destination)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result?;
    // Publication was fully hashed above. Seed the receipt for the next launch;
    // include the final path's metadata after rename, not the staging path.
    if let Ok(stamp) = verify_tree(&destination, files, false) {
        save_receipt(&destination, &bytes, &stamp);
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> Scratch {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "horizon-ios-import-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("source")).unwrap();
        Scratch(root)
    }
    fn index(root: &Path, version: i32) {
        let manifest = root.join("horizon-resources.json");
        fs::write(
            &manifest,
            serde_json::json!({"schema":7,"data_version":version,"minecraft":horizon_core::VANILLA_VERSION}).to_string(),
        )
        .unwrap();
        fs::write(root.join("mobile-import.json"), serde_json::json!({"format":1,"files":{"horizon-resources.json":digest(&manifest).unwrap()}}).to_string()).unwrap();
    }
    #[test]
    fn import_is_atomic_repeatable_and_detects_corruption() {
        let t = fixture();
        let source = t.0.join("source");
        let storage = t.0.join("storage");
        index(&source, horizon_core::version::DATA_VERSION);
        let imported = import(&source, &storage).unwrap();
        assert_eq!(import(&source, &storage).unwrap(), imported);
        fs::write(imported.join("horizon-resources.json"), b"corrupt").unwrap();
        assert!(import(&source, &storage).is_err());
        assert!(!storage.join(".incoming").exists());
    }
    #[test]
    fn rejects_wrong_version_and_unsafe_paths_before_copying() {
        let t = fixture();
        let source = t.0.join("source");
        let storage = t.0.join("storage");
        index(&source, -1);
        assert!(import(&source, &storage).is_err());
        assert!(!storage.exists());
        for name in ["../escape", "/absolute", "a/../../escape", "a\\escape", ""] {
            assert!(relative(name).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn rejects_links_in_shared_resource_directories() {
        use std::os::unix::fs::symlink;
        let t = fixture();
        let source = t.0.join("source");
        index(&source, horizon_core::version::DATA_VERSION);
        let external = t.0.join("outside");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("texture"), b"original test bytes").unwrap();
        symlink(&external, source.join("textures")).unwrap();
        let path = source.join("mobile-import.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["files"]["textures/texture"] =
            Value::String(digest(&external.join("texture")).unwrap());
        fs::write(path, value.to_string()).unwrap();
        assert!(import(&source, &t.0.join("storage")).is_err());
        assert!(!t.0.join("storage").exists());
    }
    #[test]
    fn warm_import_verifies_destination_without_rereading_source_payload() {
        let t = fixture();
        let source = t.0.join("source");
        let storage = t.0.join("storage");
        index(&source, horizon_core::version::DATA_VERSION);
        let imported = import(&source, &storage).unwrap();
        fs::remove_file(source.join("horizon-resources.json")).unwrap();
        assert_eq!(import(&source, &storage).unwrap(), imported);
        fs::write(imported.join("horizon-resources.json"), b"corrupt").unwrap();
        assert!(import(&source, &storage).is_err());
    }
    #[test]
    fn picker_keeps_the_previous_verified_import_on_failure_and_reopens_without_source() {
        let t = fixture();
        let folder = t.0.join("source");
        let storage = t.0.join("storage");
        index(&folder, horizon_core::version::DATA_VERSION);
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(folder.join(INDEX)).unwrap()).unwrap();
        for name in [
            "facts.json",
            "block-behaviours.json",
            "block-tags.json",
            "gameplay-facts.json",
            "assets/minecraft/test",
            "data/minecraft/test",
            "worldgen/test",
        ] {
            let path = folder.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"fixture bytes").unwrap();
            manifest["files"][name] = Value::String(digest(&path).unwrap());
        }
        fs::write(folder.join(INDEX), manifest.to_string()).unwrap();
        let installed = install(&folder, &storage).unwrap();
        assert_eq!(install(&folder, &storage).unwrap(), installed);
        index(&folder, -1);
        assert_eq!(
            install(&folder, &storage).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        fs::remove_dir_all(folder).unwrap();
        assert_eq!(available(&t.0, &t.0, &storage).unwrap(), installed);
        assert_eq!(
            fs::read(installed.join("facts.json")).unwrap(),
            b"fixture bytes"
        );
    }

    #[test]
    fn receipt_rejects_same_size_edits_even_when_mtime_is_restored() {
        let t = fixture();
        let source = t.0.join("source");
        index(&source, horizon_core::version::DATA_VERSION);
        fs::write(source.join("payload"), b"fixture").unwrap();
        let mut value: Value =
            serde_json::from_slice(&fs::read(source.join(INDEX)).unwrap()).unwrap();
        value["files"]["payload"] = Value::String(digest(&source.join("payload")).unwrap());
        fs::write(source.join(INDEX), value.to_string()).unwrap();
        let storage = t.0.join("storage");
        let root = import(&source, &storage).unwrap();
        let path = root.join("payload");
        let old_time = fs::metadata(&path).unwrap().modified().unwrap();
        let original = fs::read(&path).unwrap();
        // Same length and restored mtime: ctime must still reject the edit.
        fs::write(&path, b"changed").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old_time))
            .unwrap();
        assert!(import(&source, &storage).is_err());
        fs::write(&path, original).unwrap();
        assert_eq!(import(&source, &storage).unwrap(), root);
        // A missing or torn receipt only costs a full verification.
        fs::write(root.with_extension("verified-v1"), b"partial").unwrap();
        assert_eq!(import(&source, &storage).unwrap(), root);
    }

    #[cfg(unix)]
    #[test]
    fn warm_receipt_rejects_replaced_parent_directory() {
        use std::os::unix::fs::symlink;
        let t = fixture();
        let source = t.0.join("source");
        index(&source, horizon_core::version::DATA_VERSION);
        fs::create_dir(source.join("nested")).unwrap();
        fs::write(source.join("nested/file"), b"fixture").unwrap();
        let mut value: Value =
            serde_json::from_slice(&fs::read(source.join(INDEX)).unwrap()).unwrap();
        value["files"]["nested/file"] = Value::String(digest(&source.join("nested/file")).unwrap());
        fs::write(source.join(INDEX), value.to_string()).unwrap();
        let storage = t.0.join("storage");
        let root = import(&source, &storage).unwrap();
        fs::rename(root.join("nested"), t.0.join("moved")).unwrap();
        symlink(t.0.join("moved"), root.join("nested")).unwrap();
        assert!(import(&source, &storage).is_err());
    }

    #[test]
    fn missing_resources_are_an_error() {
        let t = fixture();
        assert!(import(&t.0.join("source"), &t.0.join("storage")).is_err());
    }
    #[test]
    fn documentation_source_is_used_when_the_bundle_has_none() {
        let t = fixture();
        let bundle = t.0.join("bundle");
        let documents = t.0.join("documents");
        fs::create_dir_all(bundle.join("PreparedResources")).unwrap();
        fs::create_dir_all(documents.join(SHARED_FOLDER)).unwrap();
        index(
            &documents.join(SHARED_FOLDER),
            horizon_core::version::DATA_VERSION,
        );
        assert_eq!(
            source(&bundle, &documents).unwrap(),
            documents.join(SHARED_FOLDER)
        );
        let imported = import_from(&bundle, &documents, &t.0.join("storage")).unwrap();
        assert!(imported.join("horizon-resources.json").is_file());
    }
    #[test]
    fn bundled_source_wins_and_absent_sources_explain_where_to_copy() {
        let t = fixture();
        let bundle = t.0.join("bundle");
        let documents = t.0.join("documents");
        fs::create_dir_all(bundle.join("PreparedResources")).unwrap();
        index(
            &bundle.join("PreparedResources"),
            horizon_core::version::DATA_VERSION,
        );
        assert_eq!(
            source(&bundle, &documents).unwrap(),
            bundle.join("PreparedResources")
        );
        fs::remove_file(bundle.join("PreparedResources/mobile-import.json")).unwrap();
        let error = source(&bundle, &documents).unwrap_err().to_string();
        assert!(error.contains(SHARED_FOLDER), "{error}");
    }
}
