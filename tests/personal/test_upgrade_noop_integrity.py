"""No-op installation must verify fresh inputs, not only the pre-check snapshot."""
from pathlib import Path
import plistlib
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import upgrade_personal as upgrade


@unittest.skipIf(upgrade.fcntl is None, "personal installer requires POSIX locks")
class NoopIntegrityTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        self.source, self.target = root / "build.app", root / "installed.app"
        self.home, self.backups = root / "home", root / "backups"
        (self.source / "Contents/MacOS").mkdir(parents=True)
        (self.source / "Contents/Info.plist").write_bytes(plistlib.dumps({
            "CFBundleIdentifier": upgrade.BUNDLE_ID,
            "CFBundleShortVersionString": "0.3.4",
            "CFBundleExecutable": "personal",
        }))
        (self.source / "Contents/MacOS/personal").write_bytes(b"same binary")
        shutil.copytree(self.source, self.target)
        (self.home / "data").mkdir(parents=True)
        self.config = self.home / "data/profiles.json"
        self.config.write_text('{"profiles":[]}', encoding="utf-8")

    def validate(self, app, home, version):
        upgrade.metadata(app, version)
        return {"native_deserialization": True, "services_started": False,
                "configuration_written": False}

    def run_upgrade(self, validator=None):
        return upgrade.upgrade(self.source, self.target, self.home, self.backups,
                               "0.3.4", apply=True, validator=validator or self.validate)

    def test_unchanged_noop_has_exact_binary_and_configuration_identity(self):
        result = self.run_upgrade()
        self.assertEqual(result["status"], "already_installed")
        self.assertEqual(result["executable_sha256"],
                         upgrade.sha(self.target / "Contents/MacOS/personal"))
        self.assertTrue(result["exact_configuration_preserved"])
        self.assertFalse(result["running_process_restarted"])
        self.assertEqual(list(self.backups.glob("*/previous.app")), [])

    def test_noop_configuration_drift_is_rejected_without_restoring_old_data(self):
        def changed(app, home, version):
            self.config.write_text('{"profiles":[],"concurrent":true}', encoding="utf-8")
            return self.validate(app, home, version)
        with self.assertRaisesRegex(ValueError, "input changed"):
            self.run_upgrade(changed)
        self.assertIn("concurrent", self.config.read_text())
        self.assertEqual(list(self.backups.glob("*/previous.app")), [])

    def test_noop_source_drift_is_rejected(self):
        def changed(app, home, version):
            (self.source / "Contents/MacOS/personal").write_bytes(b"new build")
            return self.validate(app, home, version)
        with self.assertRaisesRegex(ValueError, "input changed"):
            self.run_upgrade(changed)
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"same binary")

    def test_noop_target_drift_is_rejected_without_undoing_another_writer(self):
        def changed(app, home, version):
            (self.target / "Contents/MacOS/personal").write_bytes(b"other installation")
            return self.validate(app, home, version)
        with self.assertRaisesRegex(ValueError, "input changed"):
            self.run_upgrade(changed)
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"other installation")

    def test_noop_validator_error_is_not_success(self):
        def fail(app, home, version):
            raise ValueError("native validation failed")
        with self.assertRaisesRegex(ValueError, "native validation failed"):
            self.run_upgrade(fail)
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"same binary")


if __name__ == "__main__":
    unittest.main()
