"""Regression tests for GUI/minimal-PATH developer entry points."""
from pathlib import Path
import os
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
from personal_env import toolchain_environment, toolchain_paths
from personal import environment


class ToolchainTests(unittest.TestCase):
    def test_original_configuration_is_not_mutated(self):
        base = {"PATH": "/explicit/bin", "EXAMPLE_SECRET": "do-not-log"}
        before = dict(base)
        with tempfile.TemporaryDirectory() as directory:
            toolchain_environment(base, home=Path(directory), system="Linux")
        self.assertEqual(base, before)

    def test_installed_cargo_is_appended_after_user_path(self):
        with tempfile.TemporaryDirectory(prefix="tools with spaces ") as directory:
            home = Path(directory)
            cargo = home / ".cargo" / "bin"
            cargo.mkdir(parents=True)
            env = toolchain_environment({"PATH": "/explicit/bin"}, home=home, system="Linux")
            paths = env["PATH"].split(os.pathsep)
            self.assertEqual(paths[0], "/explicit/bin")
            self.assertIn(str(cargo), paths)

    def test_missing_directories_are_not_injected(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            env = toolchain_environment({"PATH": "/explicit/bin"}, home=home, system="Linux")
            self.assertNotIn(str(home / ".cargo" / "bin"), env["PATH"])

    def test_empty_and_duplicate_entries_are_removed(self):
        base = {"PATH": os.pathsep.join(["/explicit/bin", "", "/explicit/bin"])}
        with tempfile.TemporaryDirectory() as directory:
            env = toolchain_environment(base, home=Path(directory), system="Linux")
        paths = env["PATH"].split(os.pathsep)
        self.assertNotIn("", paths)
        self.assertEqual(paths.count("/explicit/bin"), 1)

    def test_toolchain_report_does_not_contain_environment_values(self):
        with patch("personal_env.shutil.which", return_value=None):
            report = toolchain_paths({"PATH": "/empty", "EXAMPLE_SECRET": "do-not-log"})
        self.assertEqual(set(report), {"cargo", "rustc", "node", "npm", "npx", "git"})
        self.assertNotIn("do-not-log", repr(report))

    def test_parent_environment_stays_unchanged(self):
        before = dict(os.environ)
        toolchain_environment()
        self.assertEqual(dict(os.environ), before)

    def test_personal_environment_is_separate_and_never_autostarts_legacy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env = environment(root)
            self.assertEqual(env["CODING_TOOLS_PERSONAL_HOME"], str(root / ".personal-home"))
            self.assertEqual(env["CODING_TOOLS_PERSONAL_IMPORT"], "off")
            self.assertEqual(env["CARGO_TARGET_DIR"], str(root / ".artifacts" / "cargo"))
            self.assertFalse((root / ".personal-home").exists())


if __name__ == "__main__":
    unittest.main()
