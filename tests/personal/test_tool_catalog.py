from pathlib import Path
import contextlib
import importlib.util
import io
import json
import tempfile
import unittest

PATH = Path(__file__).resolve().parents[2] / "scripts/check_tool_catalog.py"
SPEC = importlib.util.spec_from_file_location("catalog_checker", PATH)
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


def tool(name="apply_patch", fields=("patch", "request_id", "expected_hashes")):
    return {"name":name, "inputSchema":{"type":"object", "properties":{k:{"type":"string"} for k in fields}}}


class CatalogTest(unittest.TestCase):
    def test_missing_tools_and_protection_fields_are_mismatch(self):
        result = checker.compare([tool(), tool("task_open")], [tool(fields=("patch",))])
        self.assertEqual(result["status"], "mismatch")
        self.assertEqual(result["missing_tools"], ["task_open"])
        self.assertEqual(result["missing_fields"]["apply_patch"], ["expected_hashes", "request_id"])

    def test_field_only_match_does_not_claim_schema_or_live_refresh(self):
        value = {"input_fields":{"apply_patch":["patch","request_id","expected_hashes"]}}
        result = checker.compare([tool()], {"tool_contract":value})
        self.assertEqual(result["status"], "fields_match_schema_unverified")
        self.assertIsNone(result["catalog_snapshot_matches"])
        self.assertFalse(result["runtime_client_refresh_verified"])

    def test_full_schema_match(self):
        result = checker.compare({"tools":[tool()]}, {"jsonrpc":"2.0", "result":{"tools":[tool()]}})
        self.assertEqual(result["status"], "schema_match")
        self.assertTrue(result["catalog_snapshot_matches"])

    def test_schema_type_change_with_same_fields_is_rejected(self):
        changed = tool()
        changed["inputSchema"]["properties"]["patch"]["type"] = "integer"
        self.assertEqual(checker.compare([tool()], [changed])["changed_schemas"], ["apply_patch"])

    def test_required_field_change_is_rejected(self):
        changed = tool()
        changed["inputSchema"]["required"] = ["patch"]
        self.assertEqual(checker.compare([tool()], [changed])["status"], "mismatch")

    def test_empty_and_duplicate_catalogs_fail_closed(self):
        for value in [[], {}, {"input_fields":{}}, [tool(),tool()], {"input_fields":{"a":["x","x"]}}]:
            with self.assertRaises(ValueError):
                checker.catalog(value)

    def test_cli_exit_codes_and_read_only_files(self):
        with tempfile.TemporaryDirectory() as directory:
            expected, observed = Path(directory)/"expected.json", Path(directory)/"observed.json"
            expected.write_text(json.dumps([tool()]))
            for value, code in [([tool()],0), ([tool(fields=("patch",))],1), ({"input_fields":{"apply_patch":["patch","request_id","expected_hashes"]}},3), ({},2)]:
                observed.write_text(json.dumps(value))
                original = expected.read_bytes(), observed.read_bytes()
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(checker.main(["--expected",str(expected),"--observed",str(observed)]), code)
                self.assertEqual(original, (expected.read_bytes(),observed.read_bytes()))

    def test_input_byte_budget(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"large.json"
            path.write_bytes(b" " * (checker.MAX_INPUT_BYTES + 1))
            with self.assertRaises(ValueError):
                checker.read_json(path)


if __name__ == "__main__":
    unittest.main()
