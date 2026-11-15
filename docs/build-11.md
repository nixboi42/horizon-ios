# Build 11 packaging

[BUILD.json](../BUILD.json) records the toolchain, source digest, and checksum of the original unsigned release IPA. [upstream.lock](../upstream.lock) selects the frozen patch and overlay files that produced it.

On an Apple silicon Mac with Xcode, XcodeGen, Git, Python 3.11+, and the Rust iOS target installed, run from the repository root:

```sh
./build.sh --locked --check
./build.sh --locked --testflight-build 11 --testflight-unsigned --report .build/build-11.json
series=$(python3 -c 'import json; print(json.load(open("upstream.lock"))["series"])')
scripts="patches/$series/frozen-overlay/ios/scripts"
archive=dist/testflight/HorizonIOS-0.2.0-11.xcarchive
ipa=dist/HorizonIOS-0.2.0-11-rebuilt-unsigned.ipa
"$scripts/make-ipa.sh" --app "$archive/Products/Applications/HorizonIOS.app" --output "$ipa"
"$scripts/check_release.py" --archive "$ipa"
```

The builder refuses to overwrite an existing archive. The GitHub Actions IPA is a fresh build. Use the release download and its `SHA256SUMS` to verify the original published bytes.
