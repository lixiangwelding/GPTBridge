"""Compile and exercise the actual shared Rust WSL contract without Tauri/MCP.

RUSTC may select an isolated compiler. No Bridge service, credentials, application
startup, ChatGPT automation, or production access is used by these tests.
"""
from pathlib import Path
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[2]


class WslCompatTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        compiler = os.environ.get('RUSTC') or shutil.which('rustc')
        if not compiler:
            raise RuntimeError('rustc is required; use RUSTC to select an existing isolated compiler')
        cls.temp = tempfile.TemporaryDirectory(prefix='wsl-compat-test-')
        cls.addClassCleanup(cls.temp.cleanup)
        cls.directory = Path(cls.temp.name)
        source = ROOT / 'personal-runtime/src/wsl.rs'
        cls.unit = cls.directory / ('unit.exe' if os.name == 'nt' else 'unit')
        subprocess.run([compiler, '--edition', '2021', '--test', str(source), '-o', str(cls.unit)], check=True, capture_output=True, timeout=90)
        harness = cls.directory / 'probe.rs'
        harness.write_text('#[path = '+json.dumps(str(source))+'] mod wsl;\n'+r'''
fn main() {
 let a:Vec<String>=std::env::args().collect();
 match a[1].as_str() {
  "plan"=>{
   let t=wsl::WslPath::from_unc(&a[2]).expect("valid WSL root");
   for arg in t.command_args(&a[3],&a[4..],&std::env::var("WSL_TEST_TOKEN").unwrap_or("12345678-1234-1234-1234-123456789abc".into()),30000) {
    print!("{}\0",arg);
   }
  },
  "git-plan"=>{
   let target=wsl::WslPath::from_unc(&a[2]).expect("valid WSL root");
   let args:Vec<&str>=a[3..].iter().map(|x|x.as_str()).collect();
   for arg in target.git_args(&args,&std::env::var("WSL_TEST_TOKEN").unwrap_or("12345678-1234-1234-1234-123456789abc".into()),30000){print!("{}\0",arg);}
  },
  "stop"=>{wsl::CleanupPlan::from_command(std::path::Path::new(&a[2]),&a[3..]).expect("owned plan").stop().expect("owned cleanup");},
  "bootstrap"=>print!("{}",wsl::BOOTSTRAP),
  "cleanup"=>print!("{}",wsl::CLEANUP_SCRIPT),
  _=>panic!("unknown probe")
 }
}
''', encoding='utf-8')
        cls.probe = cls.directory / ('probe.exe' if os.name == 'nt' else 'probe')
        subprocess.run([compiler, '--edition', '2021', str(harness), '-o', str(cls.probe)], check=True, capture_output=True, timeout=90)

    def text(self, mode):
        return subprocess.check_output([str(self.probe), mode], text=True, timeout=10)

    def test_rust_path_and_argv_contracts(self):
        result = subprocess.run([str(self.unit)], capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        self.assertIn('0 failed', result.stdout)

    def test_exact_plan_preserves_untrusted_argument_data(self):
        args = ['-c', "print('a b')", '$(touch /tmp/not-executed)', '', 'a"b', '中文']
        output = subprocess.check_output([str(self.probe), 'plan', r'\\wsl$\Ubuntu-24.04\home\li\有 空格', 'python3', *args], timeout=10)
        plan = output.decode().split('\0')[:-1]
        self.assertEqual(plan[:5], ['--distribution', 'Ubuntu-24.04', '--cd', '/home/li/有 空格', '--exec'])
        self.assertEqual(plan[7], self.text('bootstrap'))
        self.assertEqual(plan[17:], args)
        self.assertNotIn('touch /tmp/not-executed', plan[7])

    @unittest.skipUnless(os.name == 'posix' and Path('/usr/bin/setsid').exists(), 'Linux process ownership regression')
    def test_cleanup_stops_only_owned_process_and_descendants(self):
        marker = str(uuid.uuid4())
        unrelated = subprocess.Popen(['/bin/sleep', '60'])
        self.addCleanup(lambda: unrelated.poll() is None and unrelated.kill())
        self.addCleanup(lambda: unrelated.wait(timeout=3))
        payload = 'import subprocess,time; p=subprocess.Popen(["/bin/sleep","60"]); print(p.pid,flush=True); time.sleep(60)'
        env = dict(os.environ, GPTBRIDGE_WSL_EXEC_ID=marker)
        owned = subprocess.Popen(['/usr/bin/setsid', '--wait', sys.executable, '-u', '-c', payload], env=env, stdout=subprocess.PIPE, text=True)
        def finish_owned():
            if owned.poll() is None: owned.kill()
            owned.wait(timeout=3)
            owned.stdout.close()
        self.addCleanup(finish_owned)
        child_pid = int(owned.stdout.readline().strip())
        cleanup = subprocess.run([sys.executable, '-c', self.text('cleanup'), marker], capture_output=True, text=True, timeout=5)
        self.assertEqual(cleanup.returncode, 0, cleanup.stderr)
        owned.wait(timeout=3)
        self.assertIsNone(unrelated.poll(), 'cleanup must never kill an unrelated process')
        status = Path(f'/proc/{child_pid}/stat')
        if status.exists():
            self.assertEqual(status.read_text().split(') ', 1)[1][0], 'Z', 'owned child is still running')
        unrelated.terminate()
        unrelated.wait(timeout=3)

    @unittest.skipUnless(os.name == 'posix' and Path('/usr/bin/timeout').exists(), 'Linux timeout regression')
    def test_linux_deadline_returns_timeout_status(self):
        env = dict(os.environ, GPTBRIDGE_WSL_EXEC_ID=str(uuid.uuid4()))
        start = time.monotonic()
        result = subprocess.run(['/usr/bin/setsid', '--wait', '/usr/bin/timeout', '--signal=TERM', '--kill-after=2s', '0.200s', '/bin/sleep', '60'], env=env, timeout=4)
        self.assertEqual(result.returncode, 124)
        self.assertLess(time.monotonic()-start, 4)

    def test_policy_and_both_backends_share_resolver(self):
        # Structural integration guard complements executable pure-module tests.
        source = (ROOT/'src-tauri/src/tools/exec.rs').read_text()
        self.assertEqual(source.count('parse_and_resolve(cmd,'), 3)
        self.assertIn('split_for_workspace(cmd, workspace_root)', source)
        self.assertIn('target.command_args(&program', source)
        self.assertIn('CleanupPlan::from_command', source)
        policy = (ROOT/'src-tauri/src/tools/policy.rs').read_text()
        self.assertIn('split_for_workspace(command, root)', policy)
        self.assertIn('WORKSPACE_PATH_PROTECTED', policy)
        worker = (ROOT/'personal-runtime/src/worker.rs').read_text()
        self.assertIn('WSL_CLEANUP_FAILED', worker)
        self.assertLess(worker.index('owned.stop(); //'), worker.index('drop(guards);', worker.index('owned.stop(); //')))

    @unittest.skipUnless(os.environ.get('WSL_COMPAT_REAL_DISTRO'), 'opt-in direct Windows to WSL host probe')
    def test_real_windows_to_wsl_readonly_identity_and_toolchain(self):
        runner = os.environ.get('WSL_COMPAT_RUNNER', '/mnt/c/Windows/System32/wsl.exe')
        distro = os.environ['WSL_COMPAT_REAL_DISTRO']
        workspace = os.environ['WSL_COMPAT_REAL_ROOT']
        script = 'import os,pathlib,shutil,json; print(json.dumps({"cwd":os.getcwd(),"rule":pathlib.Path.home().joinpath(".codex/skills/shared/project-workflow.md").is_file(),"node":shutil.which("node")}))'
        result = subprocess.run([runner, '--distribution', distro, '--cd', workspace, '--exec', '/bin/bash', '-lc', self.text('bootstrap'), 'gptbridge-wsl', str(uuid.uuid4()), '/usr/bin/python3', '-c', script], capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout.strip())
        self.assertEqual(data['cwd'], workspace)
        self.assertTrue(data['rule'])
        self.assertTrue(data['node'])

    @unittest.skipUnless(os.environ.get('WSL_COMPAT_REAL_DISTRO'), 'opt-in real WSL cancellation probe')
    def test_real_wsl_relay_cancellation_and_owned_cleanup(self):
        runner = os.environ.get('WSL_COMPAT_RUNNER', '/mnt/c/Windows/System32/wsl.exe')
        distro = os.environ['WSL_COMPAT_REAL_DISTRO']
        root = os.environ['WSL_COMPAT_REAL_ROOT']
        unc = '\\\\wsl.localhost\\'+distro+root.replace('/', '\\')
        marker = str(uuid.uuid4())
        env = dict(os.environ, WSL_TEST_TOKEN=marker)
        payload = 'import subprocess,time; p=subprocess.Popen(["/bin/sleep","60"]); print(p.pid,flush=True); time.sleep(60)'
        output = subprocess.check_output([str(self.probe),'plan',unc,'/usr/bin/python3','-u','-c',payload],env=env,timeout=10)
        plan = output.decode().split('\0')[:-1]
        relay = subprocess.Popen([runner,*plan],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        def cleanup():
            subprocess.run([str(self.probe),'stop',runner,*plan],capture_output=True,timeout=8)
            if relay.poll() is None: relay.kill()
            relay.wait(timeout=5)
            relay.stdout.close(); relay.stderr.close()
        self.addCleanup(cleanup)
        # Only this harmless fixture's child is ever targeted.
        child_pid = int(relay.stdout.readline().strip())
        relay.kill(); relay.wait(timeout=5)
        result = subprocess.run([str(self.probe),'stop',runner,*plan],capture_output=True,text=True,timeout=8)
        self.assertEqual(result.returncode,0,result.stderr)
        state = Path('/proc/'+str(child_pid)+'/stat')
        if state.exists(): self.assertEqual(state.read_text().split(') ',1)[1][0],'Z')

    @unittest.skipUnless(os.environ.get('WSL_COMPAT_REAL_DISTRO'), 'opt-in real dedicated Linux Git probe')
    def test_real_dedicated_git_plan_and_shared_rule_read(self):
        runner=os.environ.get('WSL_COMPAT_RUNNER','/mnt/c/Windows/System32/wsl.exe')
        distro=os.environ['WSL_COMPAT_REAL_DISTRO']; root=os.environ['WSL_COMPAT_REAL_ROOT']
        unc='\\\\wsl.localhost\\'+distro+root.replace('/','\\')
        plan=subprocess.check_output([str(self.probe),'git-plan',unc,'rev-parse','--show-toplevel'],timeout=10).decode().split('\0')[:-1]
        git=subprocess.run([runner,*plan],capture_output=True,text=True,timeout=20)
        self.assertEqual(git.returncode,0,git.stderr)
        self.assertEqual(git.stdout.strip(),root)
        self.assertNotIn('dubious ownership',git.stderr)
        rule=Path.home()/'.codex/skills/shared/project-workflow.md'
        self.assertTrue(rule.is_file())


if __name__ == '__main__':
    unittest.main()
