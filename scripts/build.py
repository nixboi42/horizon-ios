#!/usr/bin/env python3
"""Build Horizon iOS from a pinned or freshly resolved upstream revision."""
import argparse
import copy
import fcntl
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import time
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / ".build"
LOCK = ROOT / "upstream.lock"
CACHE = BUILD / "upstream.git"
OVERLAY = ROOT / "overlay"
PATCHES = ROOT / "patches"


class BuildError(Exception):
    pass


def run(*argv, cwd=ROOT, env=None, capture=False, allow_failure=False, timeout=None):
    if argv[0] == "git":
        env = {**(os.environ if env is None else env), "GIT_TERMINAL_PROMPT": "0"}
        for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR",
                    "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"):
            env.pop(key, None)
    result = subprocess.run(
        [str(arg) for arg in argv], cwd=cwd, env=env, text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None, timeout=timeout,
    )
    if result.returncode and not allow_failure:
        if capture:
            print(result.stderr or result.stdout, file=sys.stderr)
        raise BuildError(f"{argv[0]} {argv[1]} failed (exit {result.returncode})")
    return result


def output(*argv, **kwargs):
    return run(*argv, capture=True, **kwargs).stdout.strip()


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            checksum.update(block)
    return checksum.hexdigest()


def atomic_json(path, data):
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as stream:
        json.dump(data, stream, indent=2, sort_keys=True)
        stream.write("\n")
    os.replace(stream.name, path)


def contents(directory):
    return sorted(path for path in directory.rglob("*") if path.is_file())


def overlay_files(directory):
    return {
        relative: path for path in contents(directory)
        if "__pycache__" not in (relative := path.relative_to(directory)).parts
        and path.name != ".DS_Store" and path.suffix != ".pyc"
    }


def stack(directory):
    names = [line.strip() for line in (directory / "series").read_text().splitlines()
             if line.strip() and not line.lstrip().startswith("#")]
    if not names or len(names) != len(set(names)) or any(
        not re.fullmatch(r"[\w-]+\.patch", name) for name in names
    ):
        raise BuildError("Invalid patch series")
    patches = [directory / name for name in names]
    shared_overlay = directory / "frozen-overlay" if (directory / "frozen-overlay").exists() else OVERLAY
    overlay = overlay_files(shared_overlay)
    overlay.update(overlay_files(directory / "overlay"))
    fingerprint = hashlib.sha256()
    entries = [(path.relative_to(ROOT if directory == PATCHES else directory), path)
               for path in patches]
    entries.extend((Path("overlay") / rel, path) for rel, path in sorted(overlay.items()))
    for identity, path in entries:
        if path.is_symlink():
            raise BuildError(f"Source symlink: {path}")
        fingerprint.update(f"{identity}\0{path.stat().st_mode & 0o111}\0{digest(path)}\n".encode())
    if directory != PATCHES:
        fingerprint.update(f"cargo-resolution-v1\0{digest(directory / 'base')}".encode())
    return patches, overlay, fingerprint.hexdigest()


def snapshot_series(directory, overlay, fingerprint):
    verified = PATCHES / f"verified-{fingerprint}"
    if not verified.exists():
        with tempfile.TemporaryDirectory(prefix=".promoting-", dir=PATCHES) as tmp:
            staged = Path(tmp) / "series"
            shutil.copytree(directory, staged, ignore=shutil.ignore_patterns("overlay", "frozen-overlay"))
            for rel, path in overlay.items():
                target = staged / "frozen-overlay" / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target)
            if stack(staged)[2] != fingerprint:
                raise BuildError("Verified series snapshot differs from the validated inputs")
            staged.rename(verified)
    elif stack(verified)[2] != fingerprint:
        raise BuildError("Verified series snapshot differs from the validated inputs")
    return verified.name


def cache_git(*args, **kwargs):
    return run("git", "--git-dir", CACHE, *args, **kwargs)


def resolve(args, lock, patch_base):
    if not CACHE.exists():
        cache_git("init", "--bare")
    if args.offline and not args.locked:
        raise BuildError("--offline requires --locked")
    # Pin the verified tree and the selected patch preimages in the cache.
    for key, revision in (("baseline", lock.get("baseline", lock["revision"])),
                          ("verified_base", lock["patch_base"]), ("revision", lock["revision"]),
                          ("patch_base", patch_base)):
        if not re.fullmatch(r"[0-9a-f]{40}", revision):
            raise BuildError(f"upstream.lock {key} must be a full commit SHA")
        if cache_git("cat-file", "-e", revision + "^{tree}", capture=True, allow_failure=True).returncode:
            location = args.seed or lock["upstream"]
            if args.offline and not args.seed:
                raise BuildError("Pinned source absent from cache. Supply --seed /path/to/upstream.")
            cache_git("fetch", "--depth=1", "--no-tags", location, revision, timeout=180)
        cache_git("update-ref", f"refs/horizon/{key}", revision)
    if args.locked:
        return lock.get("baseline", lock["revision"]) if args.baseline else lock["revision"]

    ref = args.ref
    if ref is None:
        head = output("git", "ls-remote", "--exit-code", lock["upstream"], "HEAD", timeout=60)
        ref, _, _ = head.partition("\t")
        if not re.fullmatch(r"[0-9a-f]{40}", ref):
            raise BuildError("Official upstream did not return a commit SHA for HEAD")
    if ref.startswith("-") or ":" in ref:
        raise BuildError(f"Invalid upstream ref {ref!r}")
    if not (re.fullmatch(r"[0-9a-f]{40}", ref)
            and cache_git("cat-file", "-e", ref + "^{tree}", capture=True, allow_failure=True).returncode == 0):
        cache_git("fetch", "--depth=1", "--no-tags", lock["upstream"], ref, timeout=180)
        ref = "FETCH_HEAD"
    revision = output("git", "--git-dir", CACHE, "rev-parse", ref + "^{commit}")
    cache_git("update-ref", "refs/horizon/selected", revision)
    return revision


def patch_failure(source, revision, patch, result, conflicts, reverse, report):
    listing = run("git", "apply", "--numstat", patch, cwd=source,
                  capture=True, allow_failure=True).stdout
    patch_paths = [line.split("\t", 2)[2] for line in listing.splitlines()]
    paths = conflicts or patch_paths
    changes = {}
    base = report.get("patch_base")
    for path in paths:
        upstream = output("git", "--git-dir", CACHE, "diff", "--no-ext-diff",
                          "--unified=0", base, revision, "--", path) if base else ""
        integration = []
        match = False
        for line in patch.read_text().splitlines():
            if line.startswith("diff --git "):
                match = line.endswith(f" b/{path}")
            elif line.startswith("@@") and match:
                integration.append(line)
        category = "dependency" if Path(path).name in {"Cargo.toml", "Cargo.lock"} else (
            "architecture_review" if upstream else "context_review")
        changes[path] = {
            "category": category, "upstream_diff": upstream,
            "downstream_hunks": integration,
        }
    report["failed_patches"].append({
        "upstream": revision, "patch": patch.name,
        "diagnostics": result.stderr.strip(), "conflicted_paths": paths,
        "removal_candidate": reverse.returncode == 0, "files": changes,
    })
    report["missing_or_changed_interfaces"].extend(paths)


def lock_packages(contents):
    packages = tomllib.loads(contents)["package"]
    return {(p["name"], p["version"], p.get("source", ""), p.get("checksum", "")):
            sorted(p.get("dependencies", [])) for p in packages}


def reconcile_dependencies(source, report, offline):
    lock = source / "Cargo.lock"
    original = lock_packages(lock.read_text())
    # Cargo retains the existing resolution when it can satisfy the edited
    # manifests. The audit rejects all external identity changes, even when
    # Cargo can see newer registry versions.
    command = ("cargo", "metadata", "--format-version", "1", "--filter-platform", "aarch64-apple-ios",
               *(("--offline",) if offline else ()))
    report["dependency_status"] = "resolving"
    resolution = run(*command, cwd=source, capture=True, allow_failure=True)
    if resolution.returncode:
        raise BuildError(f"Cargo resolution failed:\n{resolution.stderr.strip()}")
    resolved = lock_packages(lock.read_text())
    removed = sorted(original.keys() - resolved.keys())
    added = sorted(resolved.keys() - original.keys())
    edges = [
        {"package": identity[:3], "before": original[identity], "after": resolved[identity]}
        for identity in sorted(original.keys() & resolved.keys())
        if original[identity] != resolved[identity]
    ]
    report["dependencies"] = {"added": added, "removed": removed, "edges": edges}
    if removed or any(identity[2] for identity in added):
        raise BuildError("Cargo changed pinned package identities. Review dependencies in the report.")
    run(*command, "--locked", cwd=source, capture=True)
    report["dependency_status"] = "passed"
    run("git", "add", "--", "Cargo.lock", cwd=source)


def apply_stack(source, revision, patches, overlay, report, resolve_cargo=False, offline=False):
    run("git", "init", "--quiet", source)
    (source / ".git/objects/info/alternates").write_text(f"{CACHE / 'objects'}\n")
    run("git", "-c", "core.hooksPath=/dev/null", "checkout", "--quiet", "--detach",
        revision, cwd=source)
    for patch in patches:
        if resolve_cargo:
            paths = run("git", "apply", "--numstat", patch, cwd=source,
                        capture=True, allow_failure=True).stdout
            if any(line.endswith("\tCargo.lock") for line in paths.splitlines()):
                report["failed_patches"].append({
                    "upstream": revision, "patch": patch.name, "conflicted_paths": ["Cargo.lock"],
                    "files": {"Cargo.lock": {"category": "dependency"}},
                })
                raise BuildError(f"{patch.name} modifies Cargo.lock. Update the manifests instead.")
        reverse = run("git", "apply", "--reverse", "--check", "--index", patch, cwd=source,
                      capture=True, allow_failure=True)
        result = run("git", "apply", "--index", "--3way", patch, cwd=source,
                     capture=True, allow_failure=True)
        unmerged = output("git", "ls-files", "-u", cwd=source)
        if result.returncode or unmerged:
            conflicts = sorted({line.split("\t", 1)[-1] for line in unmerged.splitlines()})
            patch_failure(source, revision, patch, result, conflicts, reverse, report)
            raise BuildError(f"Patch {patch.name} failed in {source}\n{result.stderr.strip()}")
        unchanged = run("git", "diff", "--cached", "--quiet", cwd=source, allow_failure=True)
        if unchanged.returncode == 0:
            patch_failure(source, revision, patch, result, [], reverse, report)
            report["failed_patches"][-1]["removal_candidate"] = True
            raise BuildError(f"Patch {patch.name} is already present. Review semantic equivalence.")
        report["applied_patches"].append(patch.name)
        # Make the next patch's no-op check relative to the preceding patch.
        run("git", "-c", "user.name=Horizon iOS Builder", "-c", "user.email=builder@invalid",
            "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false",
            "commit", "-qm", patch.stem, cwd=source)
    for rel, path in overlay.items():
        target = source / rel
        if target.exists() or target.is_symlink() or any(
            parent.is_symlink() for parent in target.parents if parent != source
            and source in parent.parents
        ):
            report["missing_or_changed_interfaces"].append(str(rel))
            raise BuildError(f"Overlay collides with upstream path {rel}")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)
        run("git", "add", "-f", "--", rel, cwd=source)
    if resolve_cargo:
        reconcile_dependencies(source, report, offline)
    report["source_tree"] = output("git", "write-tree", cwd=source)


def reconstruct(revision, patches, overlay, fingerprint, report, resolve_cargo=False, offline=False):
    source = BUILD / "source" / f"{revision}-{fingerprint[:16]}"
    receipt = source.with_suffix(".json")
    source.parent.mkdir(parents=True, exist_ok=True)
    if source.exists() and receipt.exists():
        saved = json.loads(receipt.read_text())
        clean = run("git", "diff", "--quiet", cwd=source, allow_failure=True)
        if (clean.returncode == 0 and not output("git", "ls-files", "--others",
                                                "--exclude-standard", cwd=source)
                and saved["source_tree"] == output("git", "write-tree", cwd=source)):
            report.update(saved)
            return source
    if source.exists():
        # Preserve failed checkouts and locally edited diagnostic work.
        source.rename(source.with_name(f"{source.name}.previous-{time.time_ns()}"))
    receipt.unlink(missing_ok=True)
    apply_stack(source, revision, patches, overlay, report, resolve_cargo, offline)
    atomic_json(receipt, {key: report[key] for key in ("source_tree", "applied_patches",
                                                     "dependencies", "dependency_status")
                          if key in report})
    return source


def constant(path, name, kind, pattern):
    match = re.search(rf'^pub const {name}: {kind} = ({pattern});$', path.read_text(), re.M)
    if not match:
        raise BuildError(f"Missing or changed {name} in {path.name}")
    return match[1]


def metadata(source, report):
    version_file = source / "crates/horizon-core/src/version.rs"
    prepare = source / "crates/horizon-prepare/src/lib.rs"
    mobile = source / "crates/horizon-ios/src/resources.rs"
    project = source / "ios/HorizonIOS.xcodeproj/project.pbxproj"
    try:
        version = constant(version_file, "VERSION_ID", "&str", r'"[A-Za-z0-9._-]+"')[1:-1]
        research = json.loads((source / "research-lock.json").read_text())["minecraft"]
        schema = int(constant(prepare, "SCHEMA", "i64", r"[0-9]+"))
        data_version = int(constant(version_file, "DATA_VERSION", "i32", r"[0-9]+"))
        default = deployment_default(source)
        if research["version"] != version:
            raise BuildError("Rust VERSION_ID and research-lock.json disagree")
        if f'manifest["schema"].as_i64() != Some({schema})' not in mobile.read_text():
            raise BuildError(f"iOS importer needs review for resource schema {schema}")
        for path, marker in {
            source / "Cargo.toml": '"crates/horizon-ios"',
            source / "crates/horizon-client/src/game.rs": "pub mod mobile;",
            project: "scripts/build-rust.sh",
        }.items():
            if marker not in path.read_text():
                raise BuildError(f"Missing {marker} in {path.relative_to(source)}")
    except (BuildError, OSError, ValueError, KeyError) as error:
        report["missing_or_changed_interfaces"].append(str(error))
        raise
    report["minecraft"] = {"version": version, "schema": schema, "data_version": data_version}
    return default


def deployment_default(source):
    match = re.fullmatch(r"IPHONEOS_DEPLOYMENT_TARGET = ([0-9]+\.[0-9]+)\n",
                         (source / "ios/Deployment.xcconfig").read_text())
    if not match:
        raise BuildError("Invalid ios/Deployment.xcconfig")
    return match[1]


def build_environment(args, target):
    env = dict(os.environ)
    if args.xcode:
        developer = args.xcode.resolve()
        if developer.suffix == ".app":
            developer /= "Contents/Developer"
        env["DEVELOPER_DIR"] = str(developer)
    if args.offline:
        env["CARGO_NET_OFFLINE"] = "true"
    env.pop("SDKROOT", None)
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    env["IPHONEOS_DEPLOYMENT_TARGET"] = target
    # Set the same paths used by the Xcode phase even when Xcode resets PATH.
    env["HORIZON_CARGO"] = shutil.which("cargo", path=env.get("PATH")) or ""
    if not env["HORIZON_CARGO"]:
        raise BuildError("Install Rust and rustup, then run rustup target add aarch64-apple-ios")
    return env


def verify_update_checks(report):
    checks = ("source_status", "dependency_status", "build_status", "test_status", "package_status")
    missing = [key for key in checks if report.get(key) != "passed"]
    if missing:
        raise BuildError(f"Update validation incomplete: {', '.join(missing)}")


def resource_hashes(folder):
    files = {}
    for path in sorted(folder.rglob("*")):
        if path.is_symlink():
            raise BuildError(f"Unexpected resource symlink: {path}")
        if path.is_file():
            files[path.relative_to(folder).as_posix()] = digest(path)
    return files


def resource_inputs(args, source, report, env):
    env = dict(env)
    if args.java_home:
        env["JAVA_HOME"] = str(args.java_home)
    run("cargo", "build", "--locked", "--release", "-p", "horizon-prepare", cwd=source, env=env)
    preparer = Path(env["CARGO_TARGET_DIR"]) / "release/horizon-prepare"
    if args.prepared:
        prepared = args.prepared
    else:
        options = []
        for flag, attr in (
            ("--minecraft-dir", "minecraft_dir"), ("--server-jar", "server_jar"),
            ("--client-jar", "client_jar"), ("--version-json", "version_json"),
            ("--assets-dir", "assets_dir"), ("--libraries-dir", "libraries_dir"),
        ):
            if value := getattr(args, attr):
                options.extend([flag, str(value)])
        if not args.server_jar and not env.get("HORIZON_SERVER_JAR"):
            raise BuildError("Supply the matching official server JAR with --server-jar, or use --prepared")
        # The immutable source tree covers exporters, their dependencies and pins.
        cache_key = hashlib.sha256(json.dumps([
            report["source_tree"], report["minecraft"], options,
            {key: env.get(key) for key in ("HORIZON_SERVER_JAR", "HORIZON_MINECRAFT_DIR", "JAVA_HOME")},
        ], sort_keys=True).encode()).hexdigest()
        args.resource_cache.mkdir(parents=True, exist_ok=True)
        prepared = args.resource_cache / f'{report["minecraft"]["version"]}-{cache_key}'
        if prepared.is_symlink():
            raise BuildError(f"Resource cache entry is a symlink: {prepared}")
        receipt = prepared.with_name(f"{prepared.name}.sha256.json")
        with (args.resource_cache / f"{cache_key}.lock").open("a") as mutex:
            fcntl.flock(mutex, fcntl.LOCK_EX)
            base = [preparer, "--root", source]
            if prepared.exists():
                try:
                    intact = json.loads(receipt.read_text()) == resource_hashes(prepared)
                except (BuildError, OSError, ValueError):
                    intact = False
                if not intact or run(*base, "--output", prepared, "--check",
                                     cwd=source, env=env, allow_failure=True).returncode:
                    prepared.rename(prepared.with_name(f"{prepared.name}.invalid-{time.time_ns()}"))
            if not prepared.exists():
                with tempfile.TemporaryDirectory(prefix=".preparing-", dir=args.resource_cache) as tmp:
                    stage = Path(tmp) / "output"
                    try:
                        run(*base, "--output", stage, "--prepare", "--complete", *options,
                            cwd=source, env=env)
                        run(*base, "--output", stage, "--check", cwd=source, env=env)
                    except BuildError as error:
                        raise BuildError(f"{error}. Supply official files for Minecraft "
                                         f'{report["minecraft"]["version"]} and Java '
                                         "matching research-lock.json.") from error
                    # Publish the receipt first. Readers hold the same lock and
                    # require both the final directory and matching receipt.
                    atomic_json(receipt, resource_hashes(stage))
                    stage.rename(prepared)
    return preparer, prepared


def isolated_test_environment(env, scratch):
    isolated = dict(env, HORIZON_LAUNCHER_NO_SELF_UPDATE="1", HORIZON_NO_AUDIO="1")
    for name in ("HORIZON_HOME", "HORIZON_LAUNCHER_HOME", "XDG_DATA_HOME",
                 "XDG_CONFIG_HOME", "HORIZON_TEST_GAME_DIR", "HORIZON_LAUNCHER_TRASH",
                 "HORIZON_LAUNCHER_CLIPBOARD", "HORIZON_LAUNCHER_DESKTOP"):
        folder = scratch / name
        folder.mkdir()
        isolated[name] = str(folder)
    return isolated


def run_resource_tests(source, env, prepared):
    with tempfile.TemporaryDirectory(prefix="horizon-ios-tests-") as tmp:
        scratch = Path(tmp)
        isolated = isolated_test_environment(env, scratch)
        # Client caches must stay outside the prepared resource cache.
        isolated_resources = scratch / "resources"
        if sys.platform == "darwin":
            run("cp", "-cR", prepared, isolated_resources)
        else:
            shutil.copytree(prepared, isolated_resources)
        isolated["HORIZON_TEST_RESOURCES"] = str(isolated_resources)
        for name in ("integrated_session_uses_the_same_visibility_rule",
                     "session_visibility_and_remote_packet_routing"):
            run_filtered_test(source, isolated, "horizon-client", name, "--no-default-features",
                              test_args=("--ignored", "--test-threads=1"))


def run_filtered_test(source, env, crate, name, *flags, test_args=()):
    result = run("cargo", "test", "--locked", "-p", crate, *flags, "--lib", name,
                 "--", *test_args, cwd=source, env=env, capture=True, allow_failure=True)
    print(result.stderr, end="", file=sys.stderr)
    print(result.stdout, end="", flush=True)
    if result.returncode or not re.search(r"test result: ok\. [1-9][0-9]* passed", result.stdout):
        raise BuildError(f"Required test {crate}::{name} did not pass")


def run_tests(source, env, report):
    report["test_status"] = "running"
    run(sys.executable, "-m", "unittest", "discover", "-s", ROOT / "tests", env=env)
    for crate, features in (("horizon-ios", []), ("horizon-client", ["--no-default-features"])):
        run("cargo", "check", "--locked", "-p", crate, *features, "--lib", "--target",
            "aarch64-apple-ios", cwd=source, env=env)
    with tempfile.TemporaryDirectory(prefix="horizon-ios-unit-") as tmp:
        isolated = isolated_test_environment(env, Path(tmp))
        run("cargo", "test", "--locked", "-p", "horizon-ios", "--lib", cwd=source, env=isolated)
        for name in ("game::mobile::", "mobile_server_edits_round_trip"):
            run_filtered_test(source, isolated, "horizon-client", name, "--no-default-features")
        for name in ("hotbar_uses_the_drawable", "mobile_transaction_keeps_unicode",
                     "mobile_language_snapshot"):
            run_filtered_test(source, isolated, "horizon-client-render", name)
    run(sys.executable, "-m", "unittest", "discover", "-s", "tests", "-p", "test_ios_*.py",
        cwd=source, env=env)
    report["test_status"] = "passed"


def inspect_ipa(ipa, source, env, minimum, resources):
    prefix = "Payload/HorizonIOS.app/"
    with zipfile.ZipFile(ipa) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise BuildError("Duplicate IPA entries")
        for name in names:
            parts = Path(name).parts
            if (name not in ("Payload/", prefix) and not name.startswith(prefix)
                    or ".." in parts or "\\" in name
                    or (archive.getinfo(name).external_attr >> 16) & 0o170000 == 0o120000):
                raise BuildError(f"Unexpected IPA entry: {name}")
            if any(part in {"_CodeSignature", "embedded.mobileprovision", "SC_Info", "saves"}
                   or part.lower().endswith((".jar", ".p12", ".mobileprovision"))
                   for part in parts):
                raise BuildError(f"Private or unexpected IPA entry: {name}")
        invalid = archive.testzip()
        if invalid:
            raise BuildError(f"Corrupt IPA entry: {invalid}")
        info = plistlib.loads(archive.read(prefix + "Info.plist"))
        with tempfile.TemporaryDirectory(dir=BUILD, prefix="verify-ipa-") as tmp:
            archive.extractall(tmp)
            app = Path(tmp) / prefix
            run(source / "ios/scripts/verify-app.sh", app, *(["--private"] if resources else []),
                env=env)
            if resources:
                folder = app / "PreparedResources"
                hashes = json.loads((folder / "mobile-import.json").read_text())["files"]
                if {p.relative_to(folder).as_posix(): digest(p) for p in contents(folder)
                    if p.name not in {".gitkeep", "mobile-import.json"}} != hashes:
                    raise BuildError("Packaged resource index does not match the IPA")
            elif (app / "PreparedResources").exists():
                raise BuildError("Unexpected bundled resources")
    if info["MinimumOSVersion"] != minimum:
        raise BuildError("Packaged deployment target differs from requested target")
    if not resources:
        run(sys.executable, source / "ios/scripts/check_release.py", "--archive", ipa, env=env)
    return info


def build(args, source, report, lock, target):
    env = build_environment(args, target)
    if sys.platform != "darwin":
        raise BuildError("An Apple silicon Mac with full Xcode is required for iOS builds")
    report["rust"] = output("rustc", "-vV", cwd=source, env=env)
    effective = output("rustc", "--print", "deployment-target", "--target", "aarch64-apple-ios",
                       cwd=source, env=env)
    if effective != f"IPHONEOS_DEPLOYMENT_TARGET={target}":
        raise BuildError(f"Rust selected {effective}, requested {target}")
    report["xcode"] = output("xcodebuild", "-version", env=env)
    report["sdk"] = output("xcrun", "--sdk", "iphoneos", "--show-sdk-version", env=env)
    toolchain = hashlib.sha256(json.dumps([report["rust"], report["xcode"], report["sdk"]]).encode()).hexdigest()[:16]
    env["CARGO_TARGET_DIR"] = str(BUILD / "cargo-target" / f"{target}-{toolchain}")
    deriv = source / ".local" / f"xcode-device-{target}"
    app = deriv / "Build/Products/Release-iphoneos/HorizonIOS.app"
    if report["build_status"] != "passed":
        report["build_status"] = "running"
        run(source / "ios/scripts/build-app.sh", "--derived-data", deriv,
            *(["--signed"] if args.signed else []), cwd=source, env=env)
        run(source / "ios/scripts/verify-app.sh", app, "--private", cwd=source, env=env)
        report["build_status"] = "passed"
    if args.test or args.update:
        run_tests(source, env, report)
    if args.compile_only:
        return

    report["package_status"] = "running"
    with tempfile.TemporaryDirectory(prefix="package-", dir=BUILD) as tmp:
        tmp = Path(tmp)
        if not args.public:
            preparer, prepared = resource_inputs(args, source, report, env)
            if args.test or args.update:
                report["test_status"] = "running"
                run_resource_tests(source, env, prepared)
                report["test_status"] = "passed"
            if args.bundle_resources:
                staged_app = tmp / app.name
                shutil.copytree(app, staged_app)
                app = staged_app
                resources = app / "PreparedResources"
            else:
                resources = tmp / "HorizonResources"
            run(sys.executable, source / "ios/scripts/pack-resources.py", prepared,
                "--preparer", preparer, "--dest", resources, cwd=source, env=env)
        ipa = tmp / "HorizonIOS.ipa"
        run(source / "ios/scripts/make-ipa.sh", "--app", app, "--output", ipa,
            *(["--allow-resources"] if args.bundle_resources else []), cwd=source, env=env)
        info = inspect_ipa(ipa, source, env, target, args.bundle_resources)
        mode = "public" if args.public else "personal"
        resource_index = None if args.public else digest(resources / "mobile-import.json")
        checksum = digest(ipa)
        build_id = hashlib.sha256(json.dumps(
            [report["rust"], report["xcode"], report["sdk"], target, checksum, resource_index]
        ).encode()).hexdigest()[:12]
        filename = (f'HorizonIOS-{lock["port_version"]}-{report["selected_upstream"][:12]}'
                    f'-{report["patch_stack_digest"][:12]}-{mode}-{build_id}')
        release = ROOT / "dist" / filename
        release.parent.mkdir(exist_ok=True)
        report["artifact"] = {"ipa": str(release / ipa.name), "sha256": checksum, "mode": mode,
                              "bundled_resources": args.bundle_resources,
                              "bundle_version": info["CFBundleShortVersionString"],
                              "resource_index_sha256": resource_index}
        with tempfile.TemporaryDirectory(prefix=".release-", dir=release.parent) as staging:
            export = Path(staging) / "export"
            export.mkdir(mode=0o700 if not args.public else 0o755)
            shutil.copyfile(ipa, export / ipa.name)
            (export / "SHA256SUMS").write_text(f"{checksum}  {ipa.name}\n")
            # The public manifest contains portable provenance only.
            manifest = {key: value for key, value in report.items() if key != "personal_artifact"}
            manifest = {**manifest, "package_status": "passed",
                        "artifact": {**report["artifact"], "ipa": ipa.name}}
            atomic_json(export / "build-manifest.json", manifest)
            if not args.public and not args.bundle_resources:
                shutil.move(resources, export / resources.name)
                resources = export / resources.name
            if release.exists():
                if digest(release / ipa.name) != checksum:
                    raise BuildError(f"Existing release has a different checksum: {release}")
                if resource_index and not args.bundle_resources:
                    stored = release / resources.name
                    if (digest(stored / "mobile-import.json") != resource_index
                            or resource_hashes(stored) != resource_hashes(resources)):
                        raise BuildError(f"Existing release resources differ: {stored}")
            else:
                export.rename(release)
        report["package_status"] = "passed"
        print(f"IPA: {release / ipa.name}\nSHA-256: {checksum}", flush=True)


def cli():
    if sys.version_info < (3, 11):
        raise SystemExit("Python 3.11 or newer is required. Set PYTHON to its executable.")
    parser = argparse.ArgumentParser(description=__doc__)
    select = parser.add_mutually_exclusive_group()
    select.add_argument("--latest", action="store_true", help="Build the official default branch (default)")
    select.add_argument("--locked", action="store_true", help="Build the verified commit in upstream.lock")
    select.add_argument("--baseline", action="store_true", help="Build the original verified iOS series")
    select.add_argument("--ref", help="Fetch a specific SHA, tag or branch")
    select.add_argument("--update", action="store_true", help="Build and test latest before updating upstream.lock")
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--check", action="store_true", help="Check patches, source interfaces and Cargo metadata")
    action.add_argument("--compile-only", action="store_true", help="Build an asset-free Release app without packaging")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--public", action="store_true", help="Package without Minecraft files (defaults to locked)")
    mode.add_argument("--bundle-resources", action="store_true", help="Embed owned resources in a personal IPA")
    signing = parser.add_mutually_exclusive_group()
    signing.add_argument("--unsigned", action="store_true", help="Skip signing (default)")
    signing.add_argument("--signed", action="store_true", help="Sign the intermediate app with a local development team")
    parser.add_argument("--test", action="store_true", help="Run targeted host and arm64 iOS checks")
    parser.add_argument("--expect-upstream", metavar="SHA", help="Stop if the resolved revision differs")
    parser.add_argument("--offline", action="store_true", help="Use cached Git and Cargo data with --locked")
    parser.add_argument("--seed", type=lambda s: Path(s).resolve(), help="Seed the locked revision from a local upstream clone")
    parser.add_argument("--deployment-target", default=os.environ.get("IPHONEOS_DEPLOYMENT_TARGET"))
    parser.add_argument("--xcode", type=Path, help="Xcode.app or its Contents/Developer directory")
    parser.add_argument("--report", type=lambda s: Path(s).resolve(), default=BUILD / "compatibility.json")
    for option in ("minecraft-dir", "server-jar", "client-jar", "version-json",
                   "assets-dir", "libraries-dir", "java-home", "prepared", "resource-cache"):
        parser.add_argument(f"--{option}", type=lambda s: Path(s).resolve(),
                            default=BUILD / "resources" if option == "resource-cache" else None)
    args = parser.parse_args()
    if args.baseline:
        args.locked = True
    if args.update and (args.check or args.compile_only):
        parser.error("--update requires a verified package and tests")
    if args.update and args.public:
        parser.error("--update requires matching resources and both IPA modes")
    if args.update:
        args.bundle_resources = True
    if args.check and args.test:
        parser.error("--check does source checks only. Use --compile-only --test.")
    if args.public and not (args.latest or args.update or args.ref):
        args.locked = True
    return args


def main():
    args = cli()
    BUILD.mkdir(exist_ok=True)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = {
        "selected_upstream": None, "patch_stack_digest": None, "applied_patches": [],
        "failed_patches": [], "missing_or_changed_interfaces": [], "source_status": "pending",
        "build_status": "not_run", "test_status": "not_run", "package_status": "not_run",
        "last_known_good_revision": None,
        "physical_device_validation": "not_run",
    }
    with (BUILD / "build.lock").open("a") as mutex:
        fcntl.flock(mutex, fcntl.LOCK_EX)
        try:
            lock = json.loads(LOCK.read_text())
            report["last_known_good_revision"] = lock["revision"]
            directory = PATCHES if args.baseline else PATCHES / (
                lock.get("series", "") if args.locked else "next")
            if directory.parent != PATCHES and directory != PATCHES:
                raise BuildError("Invalid verified series directory")
            patch_base = lock["patch_base"]
            if args.baseline:
                patch_base = lock.get("baseline", patch_base)
            elif directory != PATCHES:
                patch_base = (directory / "base").read_text().strip()
            report["patch_base"] = patch_base
            report["port_version"] = lock["port_version"]
            patches, overlay, fingerprint = stack(directory)
            if args.locked and not args.baseline and lock.get("patch_stack_digest", fingerprint) != fingerprint:
                raise BuildError("Verified patch series digest differs from upstream.lock")
            report["patch_stack_digest"] = fingerprint
            revision = resolve(args, lock, patch_base)
            report["selected_upstream"] = revision
            if args.expect_upstream and revision != args.expect_upstream:
                raise BuildError(f"Expected upstream {args.expect_upstream}, resolved {revision}")
            print(f"Upstream {revision}\nPatch and overlay SHA-256 {fingerprint}", flush=True)
            source = reconstruct(revision, patches, overlay, fingerprint, report,
                                 directory != PATCHES, args.offline)
            default = metadata(source, report)
            target = args.deployment_target or default
            if not re.fullmatch(r"[0-9]+\.[0-9]+", target) or tuple(map(int, target.split("."))) < (15, 0):
                raise BuildError("Specify an iOS deployment target of 15.0 or higher")
            report["deployment_target"] = target
            report["source_status"] = "passed"
            if not args.check:
                if args.update and target != default:
                    raise BuildError("--update requires the default deployment target")
                build(args, source, report, lock, target)
            if args.update:
                public = copy.copy(args)
                public.public, public.bundle_resources = True, False
                public.update, public.test = False, False
                report["personal_artifact"] = report.pop("artifact")
                build(public, source, report, lock, target)
                verify_update_checks(report)
                series = snapshot_series(directory, overlay, fingerprint)
                atomic_json(LOCK, {**lock, "baseline": lock.get("baseline", lock["revision"]),
                                   "revision": revision, "patch_base": patch_base,
                                   "series": series, "patch_stack_digest": fingerprint})
                report["last_known_good_revision"] = revision
            return 0
        except (BuildError, OSError, ValueError, KeyError, zipfile.BadZipFile,
                subprocess.TimeoutExpired) as error:
            if report["source_status"] == "pending":
                report["source_status"] = "failed"
            for key in ("build_status", "test_status", "package_status", "dependency_status"):
                if report.get(key) in {"running", "resolving"}:
                    report[key] = "failed"
            report["error"] = str(error)
            print(f"Build failed: {error}", file=sys.stderr)
            return 1
        finally:
            atomic_json(args.report, report)
            print(f"Compatibility report: {args.report}", flush=True)


if __name__ == "__main__":
    sys.exit(main())
