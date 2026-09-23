import importlib.util
import json
from pathlib import Path
import plistlib
import sqlite3
import tempfile
import unittest
from unittest.mock import patch
import uuid

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("service_personal",ROOT/"scripts/service_personal.py")
service=importlib.util.module_from_spec(spec);spec.loader.exec_module(service)

class ServiceActivationTests(unittest.TestCase):
    def test_rpc_idle_reassembles_concatenated_records_and_rejects_pending(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/"rpc.log"
            start="[rpc] request id=0 method=tools/call tool=read_file"
            end="[rpc] completed id=0 method=tools/call tool=read_file"
            for text,expected in [("",False),(start,False),(start+end,True),(end,False),(start+"[rpc] request malformed",False)]:
                path.write_text(text)
                self.assertEqual(service.rpc_idle(path),expected)
            path.write_text(start+"[rpc] worker_failed id=0 method=tools/call tool=read_file error=fixture")
            self.assertTrue(service.rpc_idle(path))
    def test_rpc_idle_does_not_invent_evidence_for_missing_or_oversized_logs(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/"rpc.log"
            self.assertFalse(service.rpc_idle(path))
            with path.open("wb") as stream:stream.truncate(32*1024*1024+1)
            self.assertFalse(service.rpc_idle(path))
    def test_launch_definition_is_local_and_has_no_auth_material(self):
        result=service.launch_definition(Path("/tmp/Personal App/native"),Path("/tmp/home"),"profile-fixture",28766)
        self.assertEqual(result["ProgramArguments"],["/tmp/Personal App/native","--personal-serve","profile-fixture","28766"])
        self.assertTrue(result["KeepAlive"])
        self.assertNotIn("token",json.dumps(result).lower())
        for profile,port in [("../bad",28766),("okay",80)]:
            with self.assertRaises(ValueError):service.launch_definition(Path("/tmp/native"),Path("/tmp/home"),profile,port)

    def test_foreign_job_inspection_is_read_only_and_includes_queued_tasks(self):
        with tempfile.TemporaryDirectory() as folder:
            home=Path(folder);workspace=home/"workspace";workspace.mkdir()
            key=service.hashlib.sha256(str(workspace.resolve()).encode()).hexdigest()
            db=home/"Library/Application Support/coding-tools-mcp/harness/personal-runtime"/key/"runtime.sqlite3"
            db.parent.mkdir(parents=True)
            with sqlite3.connect(db) as conn:
                conn.execute("CREATE TABLE jobs(id TEXT,task_id TEXT,state TEXT)")
                conn.executemany("INSERT INTO jobs VALUES(?,?,?)",[("own","mine","running"),("other","theirs","queued"),("unbound",None,"running"),("done","theirs","exited")])
            before=db.read_bytes()
            self.assertEqual(set(service.foreign_jobs(home,workspace,"mine")),{"other","unbound"})
            self.assertEqual(db.read_bytes(),before)

    def test_missing_job_store_is_not_silently_created(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(sqlite3.Error):service.foreign_jobs(Path(folder),Path(folder),"mine")

    def fixture(self,home):
        app=home/"Applications/Coding Tools MCP Personal.app/Contents"
        (app/"MacOS").mkdir(parents=True)
        (app/"Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier":"com.lixiangwelding.codingtools.personal","CFBundleShortVersionString":"0.3.6","CFBundleExecutable":"native"}))
        (app/"MacOS/native").write_text("fixture")
        config=home/"Library/Application Support/coding-tools-mcp-personal/data/profiles.json"
        config.parent.mkdir(parents=True);config.write_text(json.dumps({"profiles":[{"id":"fixture","path":str(home),"runtime":{"local_port":28766}}]}))
        repo=home/"repo";(repo/"src-tauri").mkdir(parents=True)
        (repo/"src-tauri/tauri.conf.json").write_text('{"version":"0.3.6"}')
        return repo,config

    def test_active_old_listener_is_preserved_without_explicit_handover(self):
        with tempfile.TemporaryDirectory() as folder:
            home=Path(folder);repo,config=self.fixture(home);before=config.read_bytes()
            with patch.object(service.Path,"home",return_value=home),patch.object(service,"ROOT",repo),patch.object(service.sys,"platform","darwin"),patch.object(service,"pids",return_value=[9999]),patch.object(service,"health",return_value={"version":"0.3.4"}),patch.object(service.os,"kill") as kill,patch.object(service.subprocess,"Popen") as popen:
                result=service.activate("fixture",str(uuid.uuid4()))
                self.assertEqual(result["status"],"blocked");kill.assert_not_called();popen.assert_not_called()
            self.assertEqual(config.read_bytes(),before)

    def test_matching_live_version_is_idempotent(self):
        with tempfile.TemporaryDirectory() as folder:
            home=Path(folder);repo,_=self.fixture(home)
            with patch.object(service.Path,"home",return_value=home),patch.object(service,"ROOT",repo),patch.object(service.sys,"platform","darwin"),patch.object(service,"pids",return_value=[9999]),patch.object(service,"health",return_value={"version":"0.3.6"}),patch.object(service.os,"kill") as kill:
                self.assertEqual(service.activate("fixture",str(uuid.uuid4()))["status"],"already_running")
                kill.assert_not_called()
