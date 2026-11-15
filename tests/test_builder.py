"""Exercise patch failures and cache integrity with disposable source fixtures."""
import hashlib
import contextlib
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import build


class PatchStackTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.base = self.root / "upstream"
        self.base.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Test Fixture")
        self.git("config", "commit.gpgSign", "false")
        self.git("config", "core.hooksPath", "/dev/null")
        self.file = self.base / "app.txt"
        self.file.write_text("base\n")
        self.git("add", "app.txt")
        self.git("commit", "-qm", "base")
        self.original = self.git("rev-parse", "HEAD").strip()
        self.file.write_text("mobile\n")
        self.patch = self.root / "mobile.patch"
        self.patch.write_text(self.git("diff", "--full-index"))
        self.git("add", "app.txt")
        self.git("commit", "-qm", "equivalent upstream change")
        self.equivalent = self.git("rev-parse", "HEAD").strip()
        self.git("checkout", "--quiet", "--detach", self.original)
        self.file.write_text("different upstream change\n")
        self.git("add", "app.txt")
        self.git("commit", "-qm", "conflict")
        self.conflict = self.git("rev-parse", "HEAD").strip()
        cache = patch.object(build, "CACHE", self.base / ".git")
        cache.start()
        self.addCleanup(cache.stop)
        self.report = {"applied_patches": [], "failed_patches": [], "missing_or_changed_interfaces": []}
        self.report["patch_base"] = self.original

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.base, text=True)

    def test_applies_patch_to_isolated_source(self):
        target = self.root / "applied"
        build.apply_stack(target, self.original, [self.patch], {}, self.report)
        self.assertEqual((target / "app.txt").read_text(), "mobile\n")
        self.assertEqual(self.file.read_text(), "different upstream change\n")
        self.assertEqual(self.report["applied_patches"], ["mobile.patch"])

    def test_conflict_reports_exact_patch_and_path(self):
        with self.assertRaisesRegex(build.BuildError, "mobile.patch"):
            build.apply_stack(self.root / "failed", self.conflict, [self.patch], {}, self.report)
        failure = self.report["failed_patches"][0]
        self.assertEqual(failure["upstream"], self.conflict)
        self.assertEqual(failure["conflicted_paths"], ["app.txt"])
        self.assertIn("+different upstream change", failure["files"]["app.txt"]["upstream_diff"])
        self.assertTrue(failure["files"]["app.txt"]["downstream_hunks"])
        self.assertFalse(self.report["applied_patches"])

    def test_failed_patch_never_writes_a_reconstruction_receipt(self):
        with patch.object(build, "BUILD", self.root / "build"):
            with self.assertRaises(build.BuildError):
                build.reconstruct(self.conflict, [self.patch], {}, "fixture", self.report)
        self.assertFalse(list((self.root / "build" / "source").glob("*.json")))

    def test_equivalent_patch_requires_review(self):
        with self.assertRaisesRegex(build.BuildError, "already present|failed"):
            build.apply_stack(self.root / "duplicate", self.equivalent, [self.patch], {}, self.report)
        self.assertTrue(self.report["failed_patches"][0]["removal_candidate"])

    def test_cache_receipt_detects_changed_file_and_rejects_symlink(self):
        path = self.root / "data"
        path.mkdir()
        member = path / "payload"
        member.write_bytes(b"initial")
        self.assertEqual(build.resource_hashes(path), {"payload": hashlib.sha256(b"initial").hexdigest()})
        member.write_bytes(b"corrupt")
        self.assertNotEqual(build.resource_hashes(path)["payload"], hashlib.sha256(b"initial").hexdigest())
        os.symlink(member, path / "alias")
        with self.assertRaisesRegex(build.BuildError, "resource symlink"):
            build.resource_hashes(path)

    def test_filtered_release_check_rejects_missing_test(self):
        result = subprocess.CompletedProcess([], 0, "test result: ok. 0 passed; 0 failed\n", "")
        with patch.object(build, "run", return_value=result), contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(build.BuildError, "did not pass"):
                build.run_filtered_test(self.root, {}, "fixture", "required_test")

    def test_candidate_public_build_resolves_the_candidate_base(self):
        with patch.object(sys, "argv", ["build.py", "--candidate", "--public", "--offline"]):
            args = build.cli()
        self.assertTrue(args.candidate)
        self.assertFalse(args.locked)
        lock = {"baseline": self.original, "patch_base": self.conflict, "revision": self.conflict}
        self.assertEqual(build.resolve(args, lock, self.original), self.original)

    def test_verified_series_survives_candidate_and_shared_overlay_edits(self):
        patches = self.root / "patches"
        candidate = patches / "next"
        shared = self.root / "overlay"
        candidate.mkdir(parents=True)
        shared.mkdir()
        (candidate / "series").write_text("mobile.patch\n")
        (candidate / "base").write_text(self.original)
        (candidate / "mobile.patch").write_bytes(self.patch.read_bytes())
        (shared / "host.rs").write_text("original")
        with patch.multiple(build, ROOT=self.root, PATCHES=patches, OVERLAY=shared):
            _, overlay, digest = build.stack(candidate)
            verified = patches / build.snapshot_series(candidate, overlay, digest)
            (shared / "host.rs").write_text("edited")
            (candidate / "mobile.patch").write_text("edited")
            _, verified_overlay, verified_digest = build.stack(verified)
            self.assertEqual(verified_digest, digest)
            self.assertEqual(verified_overlay[Path("host.rs")].read_text(), "original")


if __name__ == "__main__":
    unittest.main()
