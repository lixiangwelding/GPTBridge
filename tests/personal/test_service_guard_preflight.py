import contextlib
import hashlib
import io
import json
from pathlib import Path
import plistlib
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch
import uuid

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import restart_personal
import service_personal


class ServiceGuardPreflightTests(unittest.TestCase):
    def test_launchd_port_must_match_selected_profile(self):
        text = 'program = /app/native\npid = 42\narguments = {\n/app/native\n--personal-serve\nfixture\n28767\n}\n'
        self.assertEqual(42, restart_personal.parse_service(text, Path('/app/native'), 'fixture'))
        self.assertEqual(42, restart_personal.parse_service(text, Path('/app/native'), 'fixture', 28767))
        with self.assertRaises(ValueError):
            restart_personal.parse_service(text, Path('/app/native'), 'fixture', 28766)

    def test_new_app_name_is_preferred_and_legacy_install_remains_compatible(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            legacy = home / 'Applications/Coding Tools MCP Personal.app'
            preferred = home / 'Applications/GPTBridge.app'
            self.assertEqual(preferred, service_personal.personal_app(home))
            legacy.mkdir(parents=True)
            self.assertEqual(legacy, service_personal.personal_app(home))
            preferred.mkdir()
            self.assertEqual(preferred, service_personal.personal_app(home))
            explicit = home / 'Other/GPTBridge.app'
            self.assertEqual(explicit, service_personal.personal_app(home, explicit))
            with self.assertRaises(ValueError):
                service_personal.personal_app(home, Path('GPTBridge.app'))

    def test_strict_signature_rejection_is_a_preflight_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            app = Path(folder).resolve() / 'GPTBridge.app'
            app.mkdir()
            with patch.object(service_personal.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1)) as run:
                with self.assertRaises(ValueError):
                    service_personal.verify_app_signature(app)
            self.assertEqual(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(app)], run.call_args[0][0])

    def test_mapped_image_check_rejects_wrong_image_and_unreadable_process(self):
        executable = Path('/Users/fixture/Applications/GPTBridge.app/Contents/MacOS/native')
        for result in [subprocess.CompletedProcess([], 0, 'p42\nn/other/native\n', ''),
                       subprocess.CompletedProcess([], 1, '', 'unavailable')]:
            with self.subTest(result=result), patch.object(service_personal.subprocess, 'run', return_value=result):
                with self.assertRaises(ValueError):
                    service_personal.verify_mapped_executable(42, executable)
        with patch.object(service_personal.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, 'p42\nn' + str(executable) + '\n', '')):
            service_personal.verify_mapped_executable(42, executable)

    def restart_fixture(self, home, include_store):
        repo = home / 'repo'
        (repo / 'src-tauri').mkdir(parents=True)
        (repo / 'src-tauri/tauri.conf.json').write_text('{"version":"0.4.2"}')
        workspace = home / 'workspace'
        workspace.mkdir()
        app = home / 'Applications/GPTBridge.app'
        (app / 'Contents/MacOS').mkdir(parents=True)
        (app / 'Contents/Info.plist').write_bytes(plistlib.dumps({
            'CFBundleIdentifier': 'com.lixiangwelding.codingtools.personal',
            'CFBundleShortVersionString': '0.4.2', 'CFBundleExecutable': 'native'}))
        executable = app / 'Contents/MacOS/native'
        executable.write_text('fixture')
        config = home / 'Library/Application Support/coding-tools-mcp-personal/data/profiles.json'
        config.parent.mkdir(parents=True)
        config.write_text(json.dumps({'profiles': [{'id': 'fixture', 'path': str(workspace), 'runtime': {'local_port': 28766}}]}))
        definition = service_personal.launch_definition(executable, home, 'fixture', 28766)
        plist = home / 'Library/LaunchAgents' / (definition['Label'] + '.plist')
        plist.parent.mkdir(parents=True)
        plist.write_bytes(plistlib.dumps(definition))
        log = home / 'Library/Application Support/coding-tools-mcp-personal/logs/fixture/mcp-requests.log'
        log.parent.mkdir(parents=True)
        log.write_text('[rpc] request id=1 method=tools/call tool=read_file[rpc] completed id=1 method=tools/call tool=read_file')
        if include_store:
            key = hashlib.sha256(str(workspace.resolve()).encode()).hexdigest()
            database = home / 'Library/Application Support/coding-tools-mcp/harness/personal-runtime' / key / 'runtime.sqlite3'
            database.parent.mkdir(parents=True)
            writer = sqlite3.connect(database)
            writer.execute('CREATE TABLE jobs(id TEXT,task_id TEXT,state TEXT)')
            writer.execute("INSERT INTO jobs VALUES('unresolved', 'other', 'unknown')")
            writer.commit()
            writer.close()
        launchd = 'program = %s\npid = 42\narguments = {\n%s\n--personal-serve\nfixture\n28766\n}\n' % (executable, executable)
        return repo, app, config, plist, launchd

    def test_real_unknown_store_or_missing_store_blocks_main_before_any_reload(self):
        for include_store in (True, False):
            with self.subTest(include_store=include_store), tempfile.TemporaryDirectory() as folder:
                home = Path(folder)
                repo, app, config, plist, launchd = self.restart_fixture(home, include_store)
                before = (config.read_bytes(), plist.read_bytes())
                probe = Mock(pid=901)
                probe.poll.return_value = None
                output = io.StringIO()
                with patch.object(restart_personal, 'ROOT', repo), patch.object(restart_personal.Path, 'home', return_value=home), \
                        patch.object(sys, 'argv', ['restart_personal.py', '--profile', 'fixture', '--task-id', str(uuid.uuid4()), '--apply']), \
                        patch.object(restart_personal, 'verify_app_signature') as signature, \
                        patch.object(restart_personal, 'verify_mapped_executable') as mapped, \
                        patch.object(restart_personal.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, launchd, '')) as run, \
                        patch.object(restart_personal.subprocess, 'Popen', return_value=probe), \
                        patch.object(restart_personal, 'wait_health', return_value={'version': '0.4.2'}), \
                        patch.object(restart_personal, 'pids', side_effect=lambda port: [42] if port == 28766 else [901]), \
                        patch.object(restart_personal.time, 'monotonic', side_effect=[0, 11]), \
                        contextlib.redirect_stdout(output):
                    self.assertEqual(1, restart_personal.main())
                report = json.loads(output.getvalue())
                self.assertEqual('not_verified', report['status'])
                self.assertFalse(report['restartRequested'])
                self.assertFalse(any('kickstart' in call.args[0] for call in run.call_args_list))
                signature.assert_called_once_with(app)
                mapped.assert_not_called()
                probe.terminate.assert_called_once()
                self.assertEqual(before, (config.read_bytes(), plist.read_bytes()))

    def test_another_same_version_listener_cannot_be_reported_as_successful_reload(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo, app, config, plist, launchd = self.restart_fixture(home, True)
            key = hashlib.sha256(str((home / 'workspace').resolve()).encode()).hexdigest()
            database = home / 'Library/Application Support/coding-tools-mcp/harness/personal-runtime' / key / 'runtime.sqlite3'
            with sqlite3.connect(database) as writer:
                writer.execute('DELETE FROM jobs')
            reloaded = []

            def launchctl(command, **_):
                if 'kickstart' in command:
                    reloaded.append(True)
                    return subprocess.CompletedProcess(command, 0, '', '')
                return subprocess.CompletedProcess(command, 0,
                    launchd.replace('pid = 42', 'pid = 99') if reloaded else launchd, '')

            def listener_pid(port):
                if port != 28766:
                    return [901]
                return [100] if reloaded else [42]

            probe = Mock(pid=901)
            probe.poll.return_value = None
            output = io.StringIO()
            with patch.object(restart_personal, 'ROOT', repo), patch.object(restart_personal.Path, 'home', return_value=home), \
                    patch.object(sys, 'argv', ['restart_personal.py', '--profile', 'fixture', '--task-id', str(uuid.uuid4()), '--apply']), \
                    patch.object(restart_personal, 'verify_app_signature'), \
                    patch.object(restart_personal, 'verify_mapped_executable') as mapped, \
                    patch.object(restart_personal.subprocess, 'run', side_effect=launchctl), \
                    patch.object(restart_personal.subprocess, 'Popen', return_value=probe), \
                    patch.object(restart_personal, 'wait_health', return_value={'version': '0.4.2'}), \
                    patch.object(restart_personal, 'pids', side_effect=listener_pid), \
                    patch.object(restart_personal.time, 'monotonic', side_effect=[0, 11]), \
                    patch.object(restart_personal.time, 'sleep'), contextlib.redirect_stdout(output):
                self.assertEqual(1, restart_personal.main())
            report = json.loads(output.getvalue())
            self.assertEqual('not_verified', report['status'])
            self.assertEqual([True], reloaded)
            self.assertTrue(report['restartRequested'])
            mapped.assert_not_called()


if __name__ == '__main__':
    unittest.main()
