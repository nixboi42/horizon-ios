"""Mobile transfer behavior using only original fixture bytes, never game assets."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class IOSResourcePackTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="horizon-ios-pack-test-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.script = self.root / "ios/scripts/pack-resources.py"
        self.script.parent.mkdir(parents=True)
        shutil.copyfile(Path(__file__).resolve().parents[1] / "ios/scripts/pack-resources.py", self.script)
        for relative, constant in {
            "crates/horizon-core/src/version.rs": "pub const DATA_VERSION: i32 = 5030;\n",
            "crates/horizon-prepare/src/lib.rs": "pub const SCHEMA: i64 = 8;\n",
        }.items():
            path = self.root / relative
            path.parent.mkdir(parents=True)
            path.write_text(constant)
        self.source = self.root / "prepared"
        self.source.mkdir()
        (self.source / "horizon-resources.json").write_text(json.dumps({"schema": 8, "data_version": 5030}))
        self.payload = b"original mobile transfer fixture\n" * 3000
        (self.source / "payload.bin").write_bytes(self.payload)
        (self.source / "horizon-data.bin").write_bytes(b"Mac absolute-path cache excluded from transfer")
        (self.source / "horizon-sprites.cache").write_bytes(b"Mac root-bound mutable sprite cache")
        self.output = self.root / "ios/Resources/PreparedResources"
        self.output.mkdir(parents=True)
        (self.output / "existing").write_bytes(b"preserve if validation fails")
        # Isolate the external preparer; the real --check is exercised separately.
        self.preparer = self.root / "preparer"
        self.preparer.write_text("#!/bin/sh\nexit 0\n")
        self.preparer.chmod(0o700)

    def run_pack(self, *args):
        return subprocess.run([sys.executable, str(self.script), str(self.source),
                               "--preparer", str(self.preparer), *map(str, args)],
                              capture_output=True, text=True, check=False)

    def test_optional_terrain_survives_streamed_file_hashing(self):
        fixture = self.root / "terrain.nbt"
        fixture.write_bytes(b"original test terrain transfer bytes")
        result = self.run_pack("--terrain-chunk", fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.output / "payload.bin").read_bytes(), self.payload)
        self.assertEqual((self.output / "ios-terrain.nbt").read_bytes(), fixture.read_bytes())
        index = json.loads((self.output / "mobile-import.json").read_text())["files"]
        self.assertEqual(index["ios-terrain.nbt"], hashlib.sha256(fixture.read_bytes()).hexdigest())
        self.assertEqual(index["payload.bin"], hashlib.sha256(self.payload).hexdigest())
        self.assertFalse((self.output / "horizon-data.bin").exists())
        self.assertTrue((self.source / "horizon-data.bin").exists())
        self.assertFalse((self.output / "horizon-sprites.cache").exists())

    def test_mobile_transfer_omits_exporter_bytecode_and_optional_audio(self):
        for name in ["classes/Exporter.class", "client-classes/Exporter.class", "assets/minecraft/sounds/test.ogg"]:
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"original test bytes")
        result = self.run_pack("--without-sounds")
        self.assertEqual(result.returncode, 0, result.stderr)
        index = json.loads((self.output / "mobile-import.json").read_text())["files"]
        self.assertFalse(any(name.endswith((".class", ".ogg")) for name in index))
        self.assertTrue((self.source / "classes/Exporter.class").exists())
        self.assertTrue((self.source / "assets/minecraft/sounds/test.ogg").exists())

    def test_failed_completeness_check_preserves_prior_staging(self):
        self.preparer.write_text("#!/bin/sh\nexit 1\n")
        result = self.run_pack()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.output / "existing").read_bytes(), b"preserve if validation fails")

    def test_without_fixture_does_not_reinterpret_hash_buffer_as_path(self):
        result = self.run_pack()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.output / "ios-terrain.nbt").exists())
        self.assertEqual((self.output / "payload.bin").read_bytes(), self.payload)


if __name__ == "__main__":
    unittest.main()
