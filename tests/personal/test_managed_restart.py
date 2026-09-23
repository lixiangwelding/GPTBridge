import importlib.util
import json
from pathlib import Path
import plistlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("managed_service",ROOT/"scripts/service_personal.py")
service=importlib.util.module_from_spec(spec);spec.loader.exec_module(service)

class ManagedRestartIdentityTests(unittest.TestCase):
    def fixture(self,home):
        definition=service.launch_definition(home/"Applications/Personal App/native",home,"profile",28766)
        path=home/"Library/LaunchAgents"/(definition["Label"]+".plist")
        path.parent.mkdir(parents=True);path.write_bytes(plistlib.dumps(definition))
        return definition,path
    def test_matching_program_profile_and_launchd_pid_are_required(self):
        with tempfile.TemporaryDirectory() as folder:
            home=Path(folder);definition,path=self.fixture(home)
            args=" ".join(definition["ProgramArguments"])
            with patch.object(service.subprocess,"run",return_value=subprocess.CompletedProcess([],0,"state = running\n pid = 3210\n","")):
                service.verify_managed_service(definition,home,3210,args)
                with self.assertRaises(ValueError):service.verify_managed_service(definition,home,3211,args)
                with self.assertRaises(ValueError):service.verify_managed_service(definition,home,3210,args+" --other")
                with self.assertRaises(ValueError):service.verify_managed_service(definition,home,3210,args.replace("profile","different"))
    def test_changed_or_symlink_launch_agent_is_preserved(self):
        with tempfile.TemporaryDirectory() as folder:
            home=Path(folder);definition,path=self.fixture(home);args=" ".join(definition["ProgramArguments"])
            altered=dict(definition,KeepAlive=False);path.write_bytes(plistlib.dumps(altered));before=path.read_bytes()
            with self.assertRaises(ValueError):service.verify_managed_service(definition,home,3210,args)
            self.assertEqual(path.read_bytes(),before)
            other=home/"other.plist";other.write_bytes(plistlib.dumps(definition));path.unlink();path.symlink_to(other)
            with self.assertRaises(ValueError):service.verify_managed_service(definition,home,3210,args)
    def test_stopped_or_wrong_launch_agent_is_not_signalled(self):
        with tempfile.TemporaryDirectory() as folder:
            home=Path(folder);definition,_=self.fixture(home)
            with patch.object(service.subprocess,"run",return_value=subprocess.CompletedProcess([],1,"","not found")),patch.object(service.os,"kill") as kill:
                with self.assertRaises(ValueError):service.verify_managed_service(definition,home,3210," ".join(definition["ProgramArguments"]))
                kill.assert_not_called()
