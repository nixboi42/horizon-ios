//! Prepared resource import. This verifies transfer integrity, not ownership or
//! authenticity. Assets must come from the player's own prepared installation.
//!
//! Two sources are accepted, in order: a developer-staged `PreparedResources`
//! folder inside the app bundle, and a `HorizonResources` folder in the app's
//! Documents directory (visible in Finder / the Files app). The second path is
//! what makes an unsigned IPA usable without redistributing Minecraft data.
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
fn relative(name: &str) -> io::Result<&Path> {
    let path = Path::new(name);
    if name.is_empty()
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
    // The signed bundle / private staging tree is stable during a pass. Check
    // shared parent directories once instead of stat-ing them for every one
    // of tens of thousands of leaves. Every file is still checked and hashed.
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
            } else if !metadata.is_file() {
                return Err(invalid("resource is not a regular file"));
            }
        }
        if Some(digest(&current)?.as_str()) != expected.as_str() {
            return Err(invalid("resource SHA-256 mismatch"));
        }
    }
    let manifest: Value = serde_json::from_slice(&fs::read(root.join("horizon-resources.json"))?)?;
    if manifest["data_version"].as_i64() != Some(horizon_core::version::DATA_VERSION.into())
        || manifest["schema"].as_i64() != Some(7)
    {
        return Err(invalid(
            "prepared resources do not match Horizon data version/schema",
        ));
    }
    Ok(())
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
pub fn import_from(bundle: &Path, documents: &Path, storage: &Path) -> io::Result<PathBuf> {
    let source = source(bundle, documents)?;
    eprintln!("RESOURCE_IMPORT source={}", source.display());
    import(&source, storage)
}

pub fn import(source: &Path, storage: &Path) -> io::Result<PathBuf> {
    let bytes = fs::read(source.join("mobile-import.json")).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "No prepared resource import: {e}. Run ios/scripts/pack-resources.py on the Mac."
            ),
        )
    })?;
    let index: Value = serde_json::from_slice(&bytes)?;
    if index["format"].as_u64() != Some(1) {
        return Err(invalid("unsupported mobile import format"));
    }
    let files = index["files"]
        .as_object()
        .ok_or_else(|| invalid("missing resource file index"))?;
    if !files.contains_key("horizon-resources.json") || files.len() > 200_000 {
        return Err(invalid("invalid resource file index"));
    }
    let started = std::time::Instant::now();
    let id = format!("{:x}", Sha256::digest(&bytes));
    let destination = storage.join(id);
    if destination.exists() {
        verify(&destination, files)?;
        eprintln!(
            "RESOURCE_IMPORT cache=hit verify_ms={:.2} files={}",
            started.elapsed().as_secs_f64() * 1000.,
            files.len()
        );
        return Ok(destination);
    }
    verify(source, files)?;
    eprintln!(
        "RESOURCE_IMPORT source_verify_ms={:.2} files={}",
        started.elapsed().as_secs_f64() * 1000.,
        files.len()
    );
    fs::create_dir_all(storage)?;
    let staging = storage.join(".incoming");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
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
        eprintln!(
            "RESOURCE_IMPORT copy_ms={:.2}",
            copying.elapsed().as_secs_f64() * 1000.
        );
        let verifying = std::time::Instant::now();
        verify(&staging, files)?;
        eprintln!(
            "RESOURCE_IMPORT publication_verify_ms={:.2}",
            verifying.elapsed().as_secs_f64() * 1000.
        );
        fs::rename(&staging, &destination)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result?;
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
            serde_json::json!({"schema":7,"data_version":version}).to_string(),
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
