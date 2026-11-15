# Internal TestFlight

This workflow is for maintainers with access to the distribution app and its Apple Developer team. Install the tools in the [build guide](building.md). Check the latest App Store Connect build number and choose a higher number for the same app version.

```sh
./build.sh --locked --testflight-build NUMBER --report .build/testflight.json
```

The archive goes to `dist/testflight/`. The version comes from `upstream.lock` and the build number comes from the command. The builder refuses to overwrite an existing archive. With cached dependencies, use `--offline`. Use `--candidate` to archive the pinned development stack.

The distribution bundle ID is `dev.horizon.ios.distribution`. TestFlight archives exclude prepared resources and development diagnostics. The build report records the upstream revision and patch digest.

Open the signed archive in Xcode Organizer and use **Distribute App → TestFlight Internal Only**. Verify the team, bundle ID, version, and build number. After processing, handle export compliance and assign the build to eligible internal testers. For an unsigned local archive, see [rebuild instructions](building.md#rebuild-the-build-11-ipa).
