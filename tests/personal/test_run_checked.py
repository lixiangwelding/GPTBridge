from pathlib import Path
import importlib.util
import os
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts"
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location("checked_runner", SCRIPTS / "run_checked.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class CheckedEnvironmentTest(unittest.TestCase):
    def test_isolated_worker_replaces_inherited_app_worker(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            inherited = {"PATH":"fixture-path", "CODING_TOOLS_PERSONAL_WORKER":"old-app"}
            with patch.object(runner, "toolchain_environment", return_value=inherited.copy()):
                env = runner.verification_environment(root, root / "run")
            suffix = ".exe" if os.name == "nt" else ""
            self.assertEqual(env["CODING_TOOLS_PERSONAL_WORKER"], str(root / ".artifacts/cargo/debug" / ("coding-tools-personal-worker" + suffix)))
            self.assertEqual(env["PATH"], "fixture-path")
            self.assertEqual(inherited["CODING_TOOLS_PERSONAL_WORKER"], "old-app")

    def test_state_and_build_are_bounded_to_test_directories(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            with patch.object(runner, "toolchain_environment", return_value={}):
                env = runner.verification_environment(root, root / "case")
            self.assertEqual(env["CODING_TOOLS_PERSONAL_HOME"], str(root / "case/home"))
            self.assertEqual(env["CODING_TOOLS_PERSONAL_IMPORT"], "off")
            self.assertEqual(env["CARGO_BUILD_JOBS"], "2")
            self.assertEqual(env["RUST_TEST_THREADS"], "2")
            self.assertTrue(Path(env["CARGO_TARGET_DIR"]).is_relative_to(root))

    def test_missing_worker_keeps_explicit_path_and_does_not_start_services(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            with patch.object(runner, "toolchain_environment", return_value={}):
                env = runner.verification_environment(root, root / "case")
            self.assertTrue(Path(env["CODING_TOOLS_PERSONAL_WORKER"]).is_absolute())
            self.assertFalse(Path(env["CODING_TOOLS_PERSONAL_WORKER"]).exists())
            self.assertEqual(list(root.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
