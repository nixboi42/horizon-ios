# Horizon iOS internal TestFlight

From the downstream checkout, use Python 3.11 or later, XcodeGen,
a supported Xcode with the iOS SDK, and the Rust `aarch64-apple-ios` target.
Sign in to Xcode with access to Apple Developer team `B8B79VPAF5`.

Build 11 was uploaded successfully. For the next internal upload, check the latest
build number in App Store Connect, then use a higher number. For example:

```sh
./build.sh --testflight-build 12 --report .build/testflight-12.json
```

Set `PYTHON` to a Python 3.11+ executable if needed. With the upstream and Cargo
dependencies already cached, add `--offline`. Add `--locked` to use the immutable
release snapshot. The default uses `patches/next/series` and the working overlays.
The builder generates the TestFlight Xcode project in the reconstructed source
directory and archives the app target.
The archive and its report record the version and build, and the report records
the upstream revision and patch digest. The archive verifier checks the bundle
metadata, platform, deployment target, app icon, and payload.

The output for build 12 would be `dist/testflight/HorizonIOS-0.2.0-12.xcarchive`.
The builder refuses to overwrite an existing archive. The app version comes from
`upstream.lock` and the build number comes from the command line.

The TestFlight scheme uses Bundle ID `dev.horizon.ios.distribution`, iOS 15.0,
and the iPhone and iPad device families. It excludes bundled prepared resources
and disables the candidate's development scenes, test sessions, and diagnostic
tracing. Users import their own prepared resources after installation. The
validation build keeps its own Bundle ID and save container.

Xcode may sign the local archive with an Apple Development identity. Organizer
applies App Store distribution signing as part of the TestFlight upload.

Open the archive in Xcode Organizer and choose **Distribute App → TestFlight
Internal Only**. Use automatic signing with team `B8B79VPAF5` and verify that
the summary shows the expected version, build number, and distribution Bundle ID before
uploading. After Apple processes the upload, resolve any export-compliance
prompt and manually add the build and eligible testers to **Horizon iOS Internal**.
