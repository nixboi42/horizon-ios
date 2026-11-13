#!/usr/bin/env python3
"""Reject game assets, credentials and build-machine details from a release.

Scans the files that would be published (tracked and untracked-but-not-ignored
Git files by default) plus, optionally, a packaged app or archive. It is a
release gate, not a malware scanner: a clean result means the specific,
well-known mistakes below are absent.

Usage:
  python3 ios/scripts/check_release.py                 # publishable Git files
  python3 ios/scripts/check_release.py --tree dist/    # an unpacked tree
  python3 ios/scripts/check_release.py --archive x.ipa # entries of a zip/ipa
  python3 ios/scripts/check_release.py --history upstream/main..HEAD
"""

import argparse
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[2]

# Mojang data must be prepared locally and never committed or shipped.
ASSET_DIRS = {
    "assets/minecraft",
    "assets/realms",
    "libraries",
    "versions",
    "runtime",
    "PreparedResources",
    "HorizonResources",
}
ASSET_SUFFIXES = {".jar", ".class", ".mca", ".mcr", ".jfr", ".hprof"}
ASSET_NAMES = {
    "horizon-data.bin",
    "client.jar",
    "server.jar",
    "bundler.jar",
    "launcher_profiles.json",
}
# A prepared folder is legal on a developer's disk but never in a release.
PREPARED_NAMES = {"mobile-import.json", "horizon-resources.json"}

SIGNING_SUFFIXES = {".ipa", ".xcarchive", ".mobileprovision", ".p12", ".cer", ".p8", ".key"}
BUILD_SUFFIXES = {".o", ".a", ".dylib", ".so", ".dll", ".exe"}

SECRET_PATTERNS = [
    ("private key", re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----")),
    ("GitHub token", re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})\b")),
    ("AWS access key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("Slack token", re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b")),
    ("OpenAI-style key", re.compile(r"\bsk-[A-Za-z0-9]{32,}\b")),
    ("Apple App Store key id", re.compile(r"\bAPP_STORE_CONNECT_(?:KEY_ID|ISSUER_ID)\s*[:=]\s*\S+")),
    ("development team id", re.compile(r"DEVELOPMENT_TEAM\s*=\s*[\"']?[A-Z0-9]{10}\b")),
]
# Documentation uses a fictional team id to show the command shape.
PLACEHOLDER_TEAM_IDS = {"ABCDE12345", "XXXXXXXXXX"}
# Absolute home paths leak the build machine's user name. The examples in
# documentation use placeholders instead.
HOME_PATH = re.compile(r"/(?:Users|home)/[A-Za-z0-9._-]+/")
WINDOWS_HOME = re.compile(r"\b[A-Za-z]:\\\\Users\\\\[A-Za-z0-9._-]+")
DEVICE_UDID = re.compile(r"\b[0-9a-f]{8}-[0-9a-f]{8}[0-9a-f]{8}\b", re.IGNORECASE)
TEAM_ID_VALUE = re.compile(r"DEVELOPMENT_TEAM\s*=\s*[\"']?([A-Z0-9]{10})\b")
BINARY_HOME_PATH = re.compile(rb"/(?:Users|home)/[A-Za-z0-9._-]+/")
BINARY_SCAN_MIN = 1024 * 1024

TEXT_LIMIT = 4 * 1024 * 1024
SKIP_DIRS = {".git", ".local", "target", "node_modules", "__pycache__", ".venv"}
IGNORED_FILES = {".DS_Store"}
# Absolute home paths are only a release defect in the files this port ships as
# documentation or automation. Upstream unit fixtures deliberately contain
# invented placeholder home paths and are not build-machine data.
SURFACE_PREFIXES = (
    "docs/ios-",
    "ios/",
    "crates/horizon-ios/",
    "tools/ios_",
    "tests/test_ios_",
    ".github/workflows/ios",
    "README.md",
    "INSTALL.md",
    "BUILDING.md",
    "CHANGELOG.md",
    "RELEASE_NOTES.md",
    "KNOWN_ISSUES.md",
    "CONTRIBUTING.md",
    "NOTICE",
    "SECURITY.md",
)


def is_surface(rel):
    return any(str(rel) == prefix or str(rel).startswith(prefix) for prefix in SURFACE_PREFIXES)


def files_in_git():
    """Tracked files plus untracked files that Git would publish."""
    result = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-co", "--exclude-standard", "-z"],
        check=True,
        capture_output=True,
    )
    return [ROOT / name for name in result.stdout.decode().split("\0") if name]


def files_in_tree(tree):
    for base, dirs, names in os.walk(tree):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
        for name in sorted(names):
            yield Path(base) / name


def relative(path, base):
    try:
        return PurePosixPath(path.relative_to(base).as_posix())
    except ValueError:
        return PurePosixPath(path.name)


def asset_problem(rel):
    """Return why a path cannot be published, or None."""
    parts = rel.parts
    if not parts:
        return None
    name = rel.name
    suffix = rel.suffix.lower()
    if suffix in ASSET_SUFFIXES:
        return "Minecraft/Java artifact"
    if suffix in SIGNING_SUFFIXES:
        return "signing material or installable package"
    if suffix in BUILD_SUFFIXES:
        return "compiled native binary"
    if name in ASSET_NAMES:
        return "prepared Minecraft data"
    if name in PREPARED_NAMES:
        return "prepared resource index"
    if parts[0] in {".local", "target"}:
        return "private build directory"
    joined = "/".join(parts)
    for directory in ASSET_DIRS:
        # Match a whole path segment sequence, not a file name substring.
        if f"/{directory}/" in f"/{joined}/":
            if rel.name == ".gitkeep":
                return None
            return f"private resource directory ({directory})"
    if "xcuserdata" in parts or name.endswith(".xcuserstate"):
        return "Xcode user state"
    if name in IGNORED_FILES:
        return "macOS metadata"
    return None


def content_problems(path, check_host_paths):
    """Yield (label, line number, excerpt) for secret-like content."""
    try:
        if path.stat().st_size > TEXT_LIMIT:
            return
        data = path.read_bytes()
    except OSError:
        return
    if b"\0" in data[:4096]:
        return
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return
    for number, line in enumerate(text.splitlines(), 1):
        for label, pattern in SECRET_PATTERNS:
            if not pattern.search(line):
                continue
            if label == "development team id":
                value = TEAM_ID_VALUE.search(line)
                if value and value.group(1) in PLACEHOLDER_TEAM_IDS:
                    continue
            yield label, number, line.strip()[:160]
        if check_host_paths:
            if HOME_PATH.search(line) or WINDOWS_HOME.search(line):
                yield "absolute home path", number, line.strip()[:160]
            if DEVICE_UDID.search(line):
                yield "device identifier", number, line.strip()[:160]


def binary_has_home_path(path):
    """Search a compiled artifact without treating it as UTF-8 text.

    Release binaries must not carry the builder's absolute workspace path in
    panic locations or dependency metadata. Short reads are text-scanned
    elsewhere; large or executable files are checked as bytes.
    """
    try:
        if path.stat().st_size < BINARY_SCAN_MIN:
            return False
        with path.open("rb") as stream:
            carry = b""
            while True:
                chunk = stream.read(1 << 20)
                if not chunk:
                    return False
                if BINARY_HOME_PATH.search(carry + chunk):
                    return True
                carry = chunk[-128:]
    except OSError:
        return False


def scan_paths(paths, base):
    findings = []
    for path in paths:
        if not path.is_file() or path.is_symlink():
            continue
        rel = relative(path, base)
        problem = asset_problem(rel)
        if problem:
            findings.append((rel, problem, ""))
            continue
        for label, number, excerpt in content_problems(path, is_surface(rel)):
            findings.append((rel, label, f"line {number}: {excerpt}"))
        try:
            executable = path.stat().st_mode & 0o111
        except OSError:
            executable = 0
        if executable and binary_has_home_path(path):
            findings.append((rel, "absolute home path", "in compiled binary"))
    return findings


def scan_archive(path):
    findings = []
    with zipfile.ZipFile(path) as archive:
        for info in archive.infolist():
            rel = PurePosixPath(info.filename)
            problem = asset_problem(rel)
            if problem:
                findings.append((rel, problem, ""))
                continue
            if info.is_dir():
                continue
            name = rel.name
            if info.file_size >= BINARY_SCAN_MIN:
                with archive.open(info) as stream:
                    carry = b""
                    while True:
                        chunk = stream.read(1 << 20)
                        if not chunk:
                            break
                        if BINARY_HOME_PATH.search(carry + chunk):
                            findings.append((rel, "absolute home path", "in compiled binary"))
                            break
                        carry = chunk[-128:]
                continue
            if info.file_size > TEXT_LIMIT:
                continue
            if name in {"Info.plist", "Localizable.strings", "embedded.mobileprovision"}:
                if name == "embedded.mobileprovision":
                    findings.append((rel, "embedded provisioning profile", ""))
                    continue
                blob = archive.read(info)
                if b"\0" in blob[:4096]:
                    continue
                for number, line in enumerate(blob.decode("utf-8", "replace").splitlines(), 1):
                    for label, pattern in SECRET_PATTERNS:
                        if pattern.search(line):
                            findings.append((rel, label, f"line {number}"))
                    if HOME_PATH.search(line) or WINDOWS_HOME.search(line):
                        findings.append((rel, "absolute home path", f"line {number}"))
    return findings


def scan_git_history(rev_range):
    result = subprocess.run(
        ["git", "-C", str(ROOT), "log", "--format=", "--name-only", "-z", rev_range, "--"],
        check=True,
        capture_output=True,
    )
    findings = []
    for raw in result.stdout.decode().split("\0"):
        name = raw.lstrip("\n")
        if not name:
            continue
        problem = asset_problem(PurePosixPath(name))
        if problem:
            findings.append((PurePosixPath(name), problem, "in history"))
    return findings


def report(findings):
    if not findings:
        print("Release privacy check passed.")
        return 0
    print("Release privacy check failed:", file=sys.stderr)
    for rel, problem, detail in findings:
        suffix = f" ({detail})" if detail else ""
        print(f"  {rel}: {problem}{suffix}", file=sys.stderr)
    print(
        "\nMove private data to .local/ (ignored by Git), remove it from the release "
        "archive, and rotate any exposed credential.",
        file=sys.stderr,
    )
    return 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--tree", type=Path, help="Scan an unpacked directory")
    mode.add_argument("--archive", type=Path, help="Scan a zip, IPA or xcarchive")
    mode.add_argument("--history", metavar="REV..REV", help="Scan paths changed in a revision range")
    args = parser.parse_args()

    if args.archive:
        findings = scan_archive(args.archive)
    elif args.history:
        findings = scan_git_history(args.history)
    elif args.tree:
        findings = scan_paths(files_in_tree(args.tree), args.tree)
    else:
        findings = scan_paths(files_in_git(), ROOT)
    return report(findings)


if __name__ == "__main__":
    sys.exit(main())
