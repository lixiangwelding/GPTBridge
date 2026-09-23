from pathlib import Path
import importlib.util
import json
import sqlite3
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("diagnose_personal", ROOT / "scripts/diagnose_personal.py")
diagnose = importlib.util.module_from_spec(spec)
spec.loader.exec_module(diagnose)


class AuditDiagnosticsTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "audit.sqlite"
        with sqlite3.connect(self.path) as connection:
            connection.execute("CREATE TABLE audit_records (tool_name TEXT,error_code TEXT,is_error INTEGER,duration_ms INTEGER,started_at_ms INTEGER,status TEXT,record_type TEXT,input_json TEXT,output_json TEXT)")
            for index in range(1, 6):
                connection.execute("INSERT INTO audit_records VALUES ('apply_patch','RESOURCE_BUSY',1,?,?, 'failure','tool','PRIVATE-CREDENTIAL','PRIVATE-OUTPUT')", (index * 10, index * 1000))

    def test_bounded_latest_sample_and_percentiles(self):
        report = diagnose.audit_summary(self.path, hours=1, limit=3, now_ms=6000)
        self.assertEqual(report["sampled_calls"], 3)
        self.assertTrue(report["truncated"])
        self.assertEqual(report["earliest_sample_ms"], 3000)
        self.assertEqual(report["latency"][0]["p50_ms"], 40)
        self.assertEqual(report["latency"][0]["p95_ms"], 50)
        self.assertEqual(report["errors"][0]["count"], 3)

    def test_readonly_and_no_raw_content(self):
        before = self.path.read_bytes()
        report = diagnose.audit_summary(self.path, now_ms=6000)
        self.assertFalse(report["truncated"])
        self.assertNotIn("PRIVATE", json.dumps(report))
        self.assertFalse(report["raw_content_exported"])
        self.assertEqual(self.path.read_bytes(), before)

    def test_missing_database_is_not_created(self):
        missing = self.path.parent / "missing.sqlite"
        with self.assertRaises(sqlite3.Error):
            diagnose.audit_summary(missing)
        self.assertFalse(missing.exists())

    def test_limits_empty_window_and_policy_classification(self):
        for hours, limit in [(0, 1), (169, 1), (1, 0), (1, 50001)]:
            with self.assertRaises(ValueError):
                diagnose.audit_summary(self.path, hours=hours, limit=limit)
        report = diagnose.audit_summary(self.path, hours=1, now_ms=10_000_000)
        self.assertEqual(report["sampled_calls"], 0)
        self.assertEqual(report["latency"], [])
        self.assertIsNone(diagnose.percentile([], .95))
        self.assertEqual(diagnose.error_category("POLICY_REJECTED"), "policy_boundary_not_automatically_a_bug")
