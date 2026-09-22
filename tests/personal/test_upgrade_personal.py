from pathlib import Path
import json
import plistlib
import shutil
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import upgrade_personal as upgrade


@unittest.skipIf(upgrade.fcntl is None, "personal macOS upgrader requires POSIX")
class UpgradeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source, self.target = self.root / "build/New.app", self.root / "apps/Personal.app"
        self.home, self.backups = self.root / "home", self.root / "backups"
        for app, version, body in [(self.source, "0.3.4", b"new"), (self.target, "0.3.3", b"old")]:
            (app / "Contents/MacOS").mkdir(parents=True)
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier":upgrade.BUNDLE_ID,
                "CFBundleShortVersionString":version, "CFBundleExecutable":"personal"}))
            (app / "Contents/MacOS/personal").write_bytes(body)
        (self.home / "data").mkdir(parents=True)
        self.config = self.home / "data/profiles.json"
        self.config.write_text('{"profiles":[{"name":"private fixture"}]}')
        self.original = self.config.read_bytes()

    def validator(self, app, home, version):
        upgrade.metadata(app, version)
        return {"profiles":1, "tool_count":33, "services_started":False, "configuration_written":False}

    def run_upgrade(self, apply=True, validator=None):
        return upgrade.upgrade(self.source, self.target, self.home, self.backups, "0.3.4",
                               apply=apply, validator=validator or self.validator)

    def test_read_only_plan_has_no_side_effects(self):
        result = self.run_upgrade(False)
        self.assertEqual(result["status"], "plan")
        self.assertFalse(self.backups.exists())
        self.assertEqual(upgrade.metadata(self.target)["CFBundleShortVersionString"], "0.3.3")

    def test_replacement_keeps_data_and_previous_app(self):
        result = self.run_upgrade()
        self.assertEqual(result["status"], "installed_app_files")
        self.assertFalse(result["running_process_restarted"])
        self.assertFalse(result["live_service_version_verified"])
        self.assertEqual(self.config.read_bytes(), self.original)
        self.assertEqual((Path(result["backup"]) / "previous.app/Contents/MacOS/personal").read_bytes(), b"old")
        self.assertEqual((Path(result["backup"]) / "personal-data/profiles.json").read_bytes(), self.original)
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"new")

    def test_identical_replay_is_a_noop(self):
        self.run_upgrade()
        before = list(self.backups.iterdir())
        self.assertEqual(self.run_upgrade()["status"], "already_installed")
        self.assertEqual(list(self.backups.iterdir()), before)

    def test_concurrent_upgrade_lock_refuses_a_second_writer(self):
        self.backups.mkdir()
        with (self.backups / ".upgrade.lock").open("w") as lock:
            upgrade.fcntl.flock(lock.fileno(), upgrade.fcntl.LOCK_EX | upgrade.fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, "in progress"):
                self.run_upgrade()
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"old")
        self.assertEqual(self.config.read_bytes(), self.original)

    def test_wrong_bundle_identity_is_rejected(self):
        p = self.source / "Contents/Info.plist"
        data = plistlib.loads(p.read_bytes()); data["CFBundleIdentifier"] = "not-personal"
        p.write_bytes(plistlib.dumps(data))
        with self.assertRaises(ValueError): self.run_upgrade()
        self.assertFalse(self.backups.exists())

    def test_stage_check_failure_leaves_existing_app(self):
        def fail(*args): raise ValueError("bad candidate")
        with self.assertRaises(ValueError): self.run_upgrade(validator=fail)
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"old")
        self.assertEqual(self.config.read_bytes(), self.original)

    def test_final_check_failure_rolls_back_app_not_data(self):
        def fail_final(app, home, version):
            if app == self.target: raise ValueError("final check failed")
            return self.validator(app, home, version)
        with self.assertRaises(ValueError): self.run_upgrade(validator=fail_final)
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"old")
        self.assertEqual(self.config.read_bytes(), self.original)
        receipt = json.loads(next(self.backups.glob("*/upgrade.json")).read_text())
        self.assertTrue(receipt["rollback_complete"])
        self.assertFalse(receipt["live_data_restored"])

    def test_concurrent_configuration_edit_is_never_overwritten(self):
        def changed(app, home, version):
            self.config.write_text('{"profiles":[],"concurrent":true}')
            return self.validator(app, home, version)
        with self.assertRaises(ValueError): self.run_upgrade(validator=changed)
        self.assertIn(b'"concurrent":true', self.config.read_bytes())
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"old")

    def test_source_drift_refuses_replacement(self):
        def changed(app, home, version):
            (self.source / "Contents/MacOS/personal").write_bytes(b"changed")
            return self.validator(app, home, version)
        with self.assertRaises(ValueError): self.run_upgrade(validator=changed)
        self.assertEqual((self.target / "Contents/MacOS/personal").read_bytes(), b"old")

    def test_configuration_symlink_is_rejected(self):
        (self.home / "data/link").symlink_to(self.source / "Contents/Info.plist")
        with self.assertRaises(ValueError): self.run_upgrade()
        self.assertFalse(self.backups.exists())

    def test_bundle_symlink_cannot_escape(self):
        (self.source / "Contents/escape").symlink_to(self.config)
        with self.assertRaises(ValueError): self.run_upgrade()
        self.assertFalse(self.backups.exists())


if __name__ == "__main__":
    unittest.main()
