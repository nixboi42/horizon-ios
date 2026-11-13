#!/usr/bin/env python3
"""Stage the owner's prepared resources for a development-only app bundle."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import subprocess
import re


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--without-sounds", action="store_true", help="Omit audio assets for the silent terrain inspector")
    parser.add_argument("--terrain-chunk", type=Path, help="Uncompressed lit 26.3 Overworld chunk NBT from a disposable world")
    parser.add_argument("--preparer", type=Path, help="Built horizon-prepare executable (defaults to target/release/horizon-prepare)")
    parser.add_argument("--dest", type=Path, help="Output folder (defaults to ios/Resources/PreparedResources). Use a HorizonResources folder to copy into the app's Documents directory.")
    args = parser.parse_args()
    try:
        source = args.source.resolve(strict=True)
        manifest = json.loads((source / "horizon-resources.json").read_text())
        terrain_chunk = args.terrain_chunk.resolve(strict=True) if args.terrain_chunk else None
    except (OSError, ValueError) as error:
        parser.error(f"Cannot read preparation/terrain input: {error}")
    if not isinstance(manifest, dict):
        parser.error("Prepared manifest must be a JSON object")
    repo = Path(__file__).resolve().parents[2]
    data = (repo / "crates/horizon-core/src/version.rs").read_text()
    schema = (repo / "crates/horizon-prepare/src/lib.rs").read_text()
    expected = {
        "schema": re.search(r"^pub const SCHEMA: i64 = (\d+);$", schema, re.M),
        "data_version": re.search(r"^pub const DATA_VERSION: i32 = (\d+);$", data, re.M),
    }
    for field, value in expected.items():
        if value is None or manifest.get(field) != int(value[1]):
            parser.error(f"Prepared resource {field} does not match this Horizon source")
    preparer = (args.preparer or repo / "target/release/horizon-prepare").resolve()
    if not preparer.is_file():
        parser.error("Build horizon-prepare on the Mac first: cargo build --release -p horizon-prepare")
    checked = subprocess.run([str(preparer), "--check", "--root", str(repo), "--output", str(source)], check=False)
    if checked.returncode:
        parser.error("horizon-prepare --check rejected the source; prepare matching complete resources first")
    output = (args.dest or Path(__file__).resolve().parents[1] / "Resources/PreparedResources").resolve()
    if output == source or output in source.parents or source in output.parents:
        parser.error("Source and bundle staging directories must be separate")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".prepared-", dir=output.parent) as tmp:
        staging = Path(tmp)
        hashes = {}
        for file in sorted(source.rglob("*")):
            if file.is_symlink():
                parser.error(f"Symlink not allowed: {file.relative_to(source)}")
            if not file.is_file():
                continue
            name = file.relative_to(source).as_posix()
            # Packed data and sprite caches are tied to the Mac resource path
            # and are rewritten by the client. Rebuild them in the sandbox;
            # indexing mutable caches would fail integrity checks on relaunch.
            if file.relative_to(source).parts[0] in {"classes", "client-classes"}:
                continue  # Mac-side preparer/exporter bytecode, never runtime assets.
            if args.without_sounds and name.startswith("assets/minecraft/sounds/"):
                continue
            if name in {"horizon-data.bin", "horizon-sprites.cache", "mobile-import.json"}:
                continue
            if file.suffix.lower() in {".jar", ".class", ".dylib", ".so", ".exe"}:
                parser.error(f"Not a prepared data file: {name}")
            target = staging / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(file, target)
            with target.open("rb") as stream:
                digest = hashlib.sha256()
                for chunk in iter(lambda: stream.read(65536), b""):
                    digest.update(chunk)
                hashes[name] = digest.hexdigest()
        if terrain_chunk:
            if terrain_chunk.stat().st_size > 64 * 1024 * 1024:
                parser.error("Terrain chunk exceeds the runtime NBT size limit")
            target = staging / "ios-terrain.nbt"
            shutil.copyfile(terrain_chunk, target)
            hashes["ios-terrain.nbt"] = hashlib.sha256(target.read_bytes()).hexdigest()
        (staging / ".gitkeep").touch()
        (staging / "mobile-import.json").write_text(json.dumps({"format": 1, "files": hashes}, sort_keys=True))
        # Publish the verified directory by rename, avoiding a second full copy.
        # Keep the previous staging tree until publication succeeds.
        with tempfile.TemporaryDirectory(prefix=".prepared-backup-", dir=output.parent) as old:
            previous = Path(old) / "PreparedResources"
            if output.exists():
                output.rename(previous)
            try:
                staging.rename(output)
            except OSError:
                if previous.exists():
                    previous.rename(output)
                raise
    print(f"Staged {len(hashes)} private files at {output}.")
    if output.name == "PreparedResources":
        print("Do not distribute an app bundle containing this folder.")


if __name__ == "__main__":
    main()
