# Build Horizon iOS

Use an Apple silicon Mac with full Xcode, Git, Python 3.11+, and Rust with the `aarch64-apple-ios` target. The release archive also needs XcodeGen. See [BUILD.json](../BUILD.json) for the original toolchain. Set `PYTHON` to a Python 3.11+ executable if necessary.

Run release commands from a checkout whose `upstream.lock` matches the release revision, patch digest, and version in [BUILD.json](../BUILD.json). Use a separate checkout of the [build 11 source ref](https://github.com/nixboi42/horizon-ios/tree/v0.2.0-beta.11) when working on a newer port.

```sh
rustup target add aarch64-apple-ios
./build.sh --locked --check
```

The check reconstructs the pinned upstream source, applies the release patches and overlay, reconciles the Cargo lock with upstream’s dependency pins, and checks source interfaces and Cargo metadata. It reports to `.build/compatibility.json`.

## Prepare game resources

Install and run Minecraft: Java Edition 26.3 once using the official launcher. Have the matching official 26.3 server JAR and JDK 25 available on the Mac. Set `JAVA_HOME` if needed.

After the source check above, run from the downstream checkout:

```sh
source_dir=$(python3 -c 'import json; b=json.load(open("BUILD.json")); print(".build/source/" + b["upstream"] + "-" + b["patch_stack_digest"][:16])')
cd "$source_dir"
cargo build --locked --release -p horizon-prepare
./target/release/horizon-prepare --prepare --complete --server-jar /absolute/path/to/26.3/server.jar
./target/release/horizon-prepare --check
python3 ios/scripts/pack-resources.py .local/horizon-client/26.3 \
  --dest "$PWD/.local/HorizonResources"
```

For a nonstandard installation, pass `--minecraft-dir` to the preparer. If Cargo writes to a custom target directory, use the resulting preparer path for preparation and pass it to the packer with `--preparer`. Use the complete preparation for audio and music. Transfer `.local/HorizonResources` to the device as described in [Installation](install.md).

## Rebuild the build 11 IPA

From the downstream checkout:

```sh
./build.sh --locked --testflight-build 11 --testflight-unsigned --report .build/build-11.json
series=$(python3 -c 'import json; print(json.load(open("upstream.lock"))["series"])')
scripts="patches/$series/frozen-overlay/ios/scripts"
archive=dist/testflight/HorizonIOS-0.2.0-11.xcarchive
ipa=dist/HorizonIOS-0.2.0-11-rebuilt-unsigned.ipa
"$scripts/make-ipa.sh" --app "$archive/Products/Applications/HorizonIOS.app" --output "$ipa"
python3 "$scripts/check_release.py" --archive "$ipa"
```

Use a fresh output location or move an existing archive aside before rebuilding. Verify the original downloadable IPA against its published checksum. Rebuilt and CI packages have their own checksums.

## Builder modes

| Option | Source or output |
| --- | --- |
| `--locked` | Release source selected by `upstream.lock` |
| `--candidate` | `patches/next` at its pinned base |
| `--latest` (default), `--ref` | Candidate stack checked against upstream HEAD or the specified ref |
| `--baseline` | Original port source and patch series |
| `--public` | Asset-free development IPA, using the locked source by default |
| `--locked --server-jar PATH` | Personal IPA and a separate `HorizonResources` folder under `dist/` |

Run `./build.sh --help` for signing, offline cache, and input options. See [patch maintenance](../patches/README.md), [TestFlight](testflight.md), and [validation](validation.md).

Keep `.build/`, `.local/`, `dist/`, prepared game files, private IPAs, and credentials out of version control.
