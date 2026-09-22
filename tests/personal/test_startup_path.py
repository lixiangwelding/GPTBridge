"""Exercise the real Rust startup helper in a fresh, single-threaded process.

The compiler uses the existing build environment. Probes inherit a minimal
macOS GUI PATH, never change the host environment or start the MCP service.
"""
from __future__ import annotations
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(sys.platform == 'darwin', 'macOS GUI startup regression')
class StartupPathProcessTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='mcp-startup-path-')
        cls.addClassCleanup(cls.temp.cleanup)
        cls.directory = Path(cls.temp.name)
        source = cls.directory / 'probe.rs'
        helper = ROOT / 'src-tauri/src/startup_path.rs'
        source.write_text('#[path = ' + json.dumps(str(helper)) + ']\nmod startup_path;\n'
            'fn main() {\n'
            ' if std::env::args().nth(1).as_deref() == Some("bootstrap") { startup_path::bootstrap(); }\n'
            ' match std::process::Command::new("node").arg("--version").output() {\n'
            '  Ok(out) => { print!("{}", String::from_utf8_lossy(&out.stdout)); std::process::exit(out.status.code().unwrap_or(125)); },\n'
            '  Err(_) => std::process::exit(127),\n'
            ' }\n}\n', encoding='utf-8')
        cls.binary = cls.directory / 'probe'
        compiler = shutil.which('rustc')
        if not compiler:
            raise RuntimeError('Run through the existing personal build/test entry: rustc is required')
        subprocess.run([compiler, '--edition', '2021', str(source), '-o', str(cls.binary)],
                       check=True, capture_output=True, timeout=90)

    def probe(self, bootstrap: bool, path: str):
        environment = dict(os.environ, PATH=path)
        return subprocess.run([str(self.binary), 'bootstrap' if bootstrap else 'unchanged'],
                              env=environment, capture_output=True, text=True, timeout=20)

    def test_actual_homebrew_node_red_green(self):
        # This is a host capability check, not a fake successful Node fixture.
        candidates = [Path('/opt/homebrew/bin/node'), Path('/usr/local/bin/node')]
        if not any(path.is_file() for path in candidates):
            self.skipTest('No installed Homebrew Node for the real-host check')
        minimal = '/usr/bin:/bin:/usr/sbin:/sbin'
        before = self.probe(False, minimal)
        self.assertEqual(before.returncode, 127, before.stdout + before.stderr)
        after = self.probe(True, minimal)
        self.assertEqual(after.returncode, 0, after.stdout + after.stderr)
        self.assertRegex(after.stdout.strip(), r'^v\d+\.\d+\.\d+')

    def test_explicit_toolchain_keeps_precedence(self):
        toolchain = self.directory / 'explicit-toolchain'
        toolchain.mkdir(exist_ok=True)
        node = toolchain / 'node'
        node.write_text('#!/bin/sh\nprintf explicit-toolchain-kept\\n\n', encoding='utf-8')
        node.chmod(0o700)
        result = self.probe(True, str(toolchain) + ':/usr/bin:/bin')
        self.assertEqual(result.returncode, 0)
        self.assertIn('explicit-toolchain-kept', result.stdout)

    def test_main_initializes_before_worker_or_runtime(self):
        main = (ROOT / 'src-tauri/src/main.rs').read_text(encoding='utf-8')
        call = main.index('startup_path::bootstrap();')
        for subsequent in ['worker::run_from_args()', 'personal_cli()', 'coding_tools_mcp_desktop_lib::run()']:
            self.assertLess(call, main.index(subsequent))


if __name__ == '__main__':
    unittest.main()
