import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import restart_personal as module


class ReloadTests(unittest.TestCase):
    def fixture(self):
        return 'program = /app/native\npid = 42\narguments = {\n/app/native\n--personal-serve\nfixture\n28766\n}\n'

    def test_exact_launchd_identity(self):
        self.assertEqual(42, module.parse_service(self.fixture(), Path('/app/native'), 'fixture'))

    def test_wrong_program_worker_or_ambiguous_pid_are_rejected(self):
        for text in (self.fixture().replace('program = /app/native', 'program = /app/other'),
                     self.fixture().replace('--personal-serve', '--personal-job-worker'),
                     self.fixture() + 'pid = 99\n', self.fixture().replace('pid = 42', 'pid = 1')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                module.parse_service(text, Path('/app/native'), 'fixture')

    def test_busy_tasks_never_restart(self):
        calls = []
        with patch.object(module.time, 'sleep'), self.assertRaises(ValueError):
            module.guarded_restart(42, lambda: 42, lambda: False, lambda: calls.append(True))
        self.assertEqual([], calls)

    def test_identity_drift_preserves_listener(self):
        calls = []
        with patch.object(module.time, 'sleep'), self.assertRaises(ValueError):
            module.guarded_restart(42, lambda: 99, lambda: True, lambda: calls.append(True))
        self.assertEqual([], calls)

    def test_new_work_before_final_step_preserves_listener(self):
        calls, samples = [], iter([True, True, False])
        with patch.object(module.time, 'sleep'), self.assertRaises(ValueError):
            module.guarded_restart(42, lambda: 42, lambda: next(samples), lambda: calls.append(True))
        self.assertEqual([], calls)

    def test_idle_exact_service_restarts_once(self):
        calls = []
        with patch.object(module.time, 'sleep'):
            module.guarded_restart(42, lambda: 42, lambda: True, lambda: calls.append(True))
        self.assertEqual([True], calls)


if __name__ == '__main__': unittest.main()
