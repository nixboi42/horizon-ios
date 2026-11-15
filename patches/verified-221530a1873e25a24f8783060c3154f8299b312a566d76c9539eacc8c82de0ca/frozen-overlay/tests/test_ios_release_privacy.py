"""Release privacy gate: original fixtures, never game assets."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_release", ROOT / "ios/scripts/check_release.py"
)
check_release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check_release)


class ReleasePrivacyTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="horizon-ios-privacy-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)

    def write(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def findings(self):
        return check_release.scan_paths(check_release.files_in_tree(self.root), self.root)

    def test_game_assets_and_signing_material_are_rejected(self):
        for name in ["game/server.jar", "assets/minecraft/textures/stone.png",
                     "PreparedResources/mobile-import.json",
                     "ios/HorizonResources/horizon-resources.json",
                     "signing/App.mobileprovision", "dist/Horizon.ipa"]:
            with self.subTest(name=name):
                self.write(name, b"original fixture bytes")
                self.assertTrue(self.findings(), name)
                (self.root / name).unlink()

    def test_bundle_placeholder_is_the_one_allowed_prepared_file(self):
        self.write("ios/Resources/PreparedResources/.gitkeep", b"")
        self.assertEqual(self.findings(), [])

    def test_credentials_are_rejected_in_any_file(self):
        self.write("crates/example/src/lib.rs",
                   b'const KEY: &str = "ghp_' + b"a" * 36 + b'";')
        self.assertEqual([finding[1] for finding in self.findings()], ["GitHub token"])

    def test_only_fictional_development_team_ids_are_allowed(self):
        self.write("BUILDING.md",
                   b"DEVELOPMENT_TEAM=ABCDE12345 ./ios/scripts/build-app.sh --signed\n")
        self.assertEqual(self.findings(), [])
        self.write("BUILDING.md",
                   b"DEVELOPMENT_TEAM=" + b"A1B2C3D4E5" + b" ./ios/scripts/build-app.sh --signed\n")
        self.assertEqual([finding[1] for finding in self.findings()], ["development team id"])

    def test_home_paths_only_fail_in_documentation_and_scripts(self):
        home = "/" + "Users/a-real-person/build.sh"
        self.write("docs/ios-build.md", ("run " + home + "\n").encode())
        self.assertEqual([finding[1] for finding in self.findings()], ["absolute home path"])
        (self.root / "docs/ios-build.md").unlink()
        fixture = 'assert_eq!(home(), "/' + 'Users/fixture/place");\n'
        self.write("crates/example/src/lib.rs", fixture.encode())
        self.assertEqual(self.findings(), [])

    def test_archive_entries_of_an_ipa_are_inspected(self):
        archive = self.root / "fixture.ipa"
        with zipfile.ZipFile(archive, "w") as package:
            package.writestr("Payload/Horizon.app/Info.plist", "<plist/>")
            package.writestr("Payload/Horizon.app/PreparedResources/mobile-import.json", "{}")
        findings = check_release.scan_archive(archive)
        self.assertEqual([finding[1] for finding in findings], ["prepared resource index"])

    def test_compiled_binaries_must_not_embed_builder_paths(self):
        payload = b"\0" * 4096 + b"/" + b"Users/real-builder/build" + b"\0" * 4096
        payload += b"\0" * (check_release.BINARY_SCAN_MIN - len(payload))
        binary = self.write("Payload/Horizon.app/HorizonIOS", payload)
        binary.chmod(0o755)
        self.assertEqual(
            [finding[1] for finding in self.findings()], ["absolute home path"]
        )
        archive = self.root / "fixture.ipa"
        with zipfile.ZipFile(archive, "w") as package:
            package.writestr("Payload/Horizon.app/HorizonIOS", payload)
        self.assertEqual(
            [finding[1] for finding in check_release.scan_archive(archive)],
            ["absolute home path"],
        )


if __name__ == "__main__":
    unittest.main()
