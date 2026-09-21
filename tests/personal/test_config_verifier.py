from pathlib import Path
import copy
import json
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
from verify_personal_config import expected_copy, verify


class ConfigVerificationTests(unittest.TestCase):
    def fixture(self):
        return {"profiles": [{"id": "fixture", "auto_start": True,
            "runtime": {"local_port": 38766, "runtime_command": "old-launcher",
                        "workspace_root": "/fixture", "upstream_mcps": [{"enabled": True}]},
            "actions": {"local_port": 12345}, "tunnel": {"type": "frp"}}],
            "shared_secrets": {"fixture_only": "not-a-real-credential"}}

    def test_transformation_preserves_source_and_only_changes_expected_fields(self):
        source = self.fixture()
        before = copy.deepcopy(source)
        result = expected_copy(source)
        self.assertEqual(source, before)
        self.assertEqual(result["shared_secrets"], before["shared_secrets"])
        self.assertEqual(result["profiles"][0]["runtime"]["local_port"], 38767)
        self.assertEqual(result["profiles"][0]["actions"]["local_port"], 38768)
        self.assertFalse(result["profiles"][0]["runtime"]["upstream_mcps"][0]["enabled"])
        self.assertFalse(result["profiles"][0]["auto_start"])

    def test_verification_returns_metadata_only_and_changes_neither_file(self):
        with tempfile.TemporaryDirectory() as directory:
            source, dest = Path(directory)/"source.json", Path(directory)/"copy.json"
            source.write_text(json.dumps(self.fixture()))
            dest.write_text(json.dumps(expected_copy(self.fixture())))
            old_source, old_dest = source.read_bytes(), dest.read_bytes()
            result = verify(source, dest)
            self.assertTrue(result["verified"])
            self.assertNotIn("not-a-real-credential", json.dumps(result))
            self.assertEqual(source.read_bytes(), old_source)
            self.assertEqual(dest.read_bytes(), old_dest)

    def test_wrong_credentials_or_reenabled_tunnel_are_rejected(self):
        for field in ("secret", "tunnel"):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as directory:
                source, dest = Path(directory)/"source.json", Path(directory)/"copy.json"
                source.write_text(json.dumps(self.fixture()))
                data = expected_copy(self.fixture())
                if field == "secret":
                    data["shared_secrets"]["fixture_only"] = "different"
                else:
                    data["profiles"][0]["tunnel"]["type"] = "frp"
                dest.write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    verify(source, dest)

    def test_missing_upstream_field_matches_native_null_normalization(self):
        source = self.fixture()
        del source["profiles"][0]["runtime"]["upstream_mcps"]
        result = expected_copy(source)
        self.assertNotIn("upstream_mcps", result["profiles"][0]["runtime"])
        self.assertNotIn("upstream_mcps", source["profiles"][0]["runtime"])

    def test_same_source_destination_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/"source.json"
            path.write_text(json.dumps(self.fixture()))
            with self.assertRaises(ValueError):
                verify(path, path)

    def test_previous_null_representation_is_accepted_but_never_rewritten(self):
        with tempfile.TemporaryDirectory() as directory:
            source, dest = Path(directory)/"source.json", Path(directory)/"copy.json"
            data = self.fixture()
            del data["profiles"][0]["runtime"]["upstream_mcps"]
            source.write_text(json.dumps(data))
            copied = expected_copy(data)
            copied["profiles"][0]["runtime"]["upstream_mcps"] = None
            dest.write_text(json.dumps(copied))
            before = dest.read_bytes()
            self.assertTrue(verify(source,dest)["verified"])
            self.assertEqual(before,dest.read_bytes())


if __name__ == "__main__":
    unittest.main()
