import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("desktop_personal",ROOT/"scripts/desktop_personal.py")
desktop=importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)

class DesktopPluginTests(unittest.TestCase):
    def test_registered_plugin_preserves_existing_app_identity(self):
        files=desktop.plugin_files("profile-fixture",Path("/tmp/personal-app"),"registered")
        self.assertEqual(json.loads(files[".app.json"])["apps"]["gptbridge"]["id"],"asdk_app_6a848a4ede608191af51370794d3091d")
        self.assertNotIn(".mcp.json",files)
        manifest=json.loads(files[".codex-plugin/plugin.json"])
        self.assertEqual(manifest["name"],"gptbridge-plugin")
        self.assertEqual(manifest["interface"]["displayName"],"GPTBridgePlugin")
        self.assertIn("skills/gptbridge-plugin/SKILL.md",files)
    def test_stdio_is_explicit_and_does_not_duplicate_remote_tools(self):
        files=desktop.plugin_files("profile-fixture",Path("/tmp/personal-app"),"stdio")
        self.assertNotIn(".app.json",files)
        self.assertNotIn("apps",json.loads(files[".codex-plugin/plugin.json"]))
        self.assertEqual(json.loads(files[".mcp.json"])["mcpServers"]["gptbridge"]["args"],["--personal-stdio","profile-fixture"])
    def test_generated_files_are_idempotent_but_do_not_clobber_edits(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/"one/two.json";desktop.write_new(path,"one");desktop.write_new(path,"one")
            with self.assertRaises(ValueError):desktop.write_new(path,"two")
            self.assertEqual(path.read_text(),"one")
    def test_invalid_profile_or_relative_executable_is_rejected(self):
        for profile,executable in [("../other",Path("/tmp/app")),("profile",Path("relative"))]:
            with self.assertRaises(ValueError):desktop.plugin_files(profile,executable,"stdio")
