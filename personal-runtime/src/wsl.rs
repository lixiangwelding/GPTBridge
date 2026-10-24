//! Shared Windows/WSL path, argv and owned-process lifecycle contract.
//! No Bridge/network calls; cleanup launches only the OS WSL runner.
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslPath {
    pub distribution: String,
    pub linux_path: String,
    unc_host: String,
}

impl WslPath {
    /// Accept both UNC providers and Windows canonical extended UNC paths.
    pub fn from_unc(raw: &str) -> Option<Self> {
        let normal = raw.replace('/', "\\");
        let normal = normal.strip_prefix(r"\\?\UNC\").map(|p| format!(r"\\{}", p)).unwrap_or(normal);
        let rest = normal.strip_prefix(r"\\")?;
        let mut parts = rest.split('\\');
        let host = parts.next()?;
        if !host.eq_ignore_ascii_case("wsl.localhost") && !host.eq_ignore_ascii_case("wsl$") { return None; }
        let distribution = parts.next()?;
        if distribution.is_empty() || !distribution.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) || distribution == "." || distribution == ".." { return None; }
        let components: Vec<_> = parts.filter(|s| !s.is_empty()).collect();
        if components.iter().any(|s| *s == "." || *s == ".." || s.contains(['\0', ':'])) { return None; }
        Some(Self { distribution: distribution.into(), linux_path: format!("/{}", components.join("/")), unc_host: host.to_string() })
    }

    /// Linux absolute paths are translated only within this distribution. The
    /// caller still applies canonicalization, scope and symlink policy.
    pub fn unc_for_linux(&self, raw: &str) -> Option<PathBuf> {
        if !raw.starts_with('/') || raw.starts_with("//") || raw.contains(['\\', '\0', ':']) { return None; }
        if raw.split('/').any(|p| p == "." || p == "..") { return None; }
        Some(PathBuf::from(format!(r"\\{}\{}{}", self.unc_host, self.distribution, raw.replace('/', "\\"))))
    }

    pub fn contains(&self, other: &Self) -> bool {
        self.distribution.eq_ignore_ascii_case(&other.distribution)
            && (other.linux_path == self.linux_path || self.linux_path == "/"
                || other.linux_path.strip_prefix(&self.linux_path).map_or(false, |tail| tail.starts_with('/')))
    }

    /// Bash reads the user's login toolchain setup, but command and arguments
    /// stay positional data, never interpolated into the shell script.
    pub fn command_args(&self, program: &str, args: &[String], token: &str, timeout_ms: u64) -> Vec<String> {
        let deadline = format!("{}.{:03}s", timeout_ms / 1000, timeout_ms % 1000);
        let mut result: Vec<String> = ["--distribution", &self.distribution, "--cd", &self.linux_path,
            "--exec", "/bin/bash", "-lc", BOOTSTRAP, "gptbridge-wsl", token,
            "/usr/bin/setsid", "--wait", "/usr/bin/timeout", "--signal=TERM", "--kill-after=2s", &deadline, program]
            .iter().map(|s| (*s).to_string()).collect();
        result.extend_from_slice(args);
        result
    }

    /// Infer the Linux user's skill directories only from a conventional home
    /// workspace. Root and /mnt workspaces do not guess a Linux user.
    pub fn skill_roots(&self) -> Vec<PathBuf> {
        let parts: Vec<_> = self.linux_path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 || parts[0] != "home" { return Vec::new(); }
        [".agents/skills", ".codex/skills"].iter()
            .filter_map(|tail| self.unc_for_linux(&format!("/home/{}/{}", parts[1], tail))).collect()
    }
}

pub fn for_path(path: &Path) -> Option<WslPath> { WslPath::from_unc(&path.to_string_lossy()) }

/// Never translate POSIX paths on native Windows workspaces or other hosts.
pub fn host_read_path(root: &Path, raw: &str) -> PathBuf {
    #[cfg(windows)]
    if let Some(target) = for_path(root) {
        if let Some(path) = target.unc_for_linux(raw) { return path; }
    }
    let _ = root;
    PathBuf::from(raw)
}

/// `$@` stays quoted; the only shell text is this fixed bootstrap. NVM is
/// optional and read from the selected Linux user's home, never Windows PATH.
pub const BOOTSTRAP: &str = "export GPTBRIDGE_WSL_EXEC_ID=\"$1\"; shift; if [ -s \"$HOME/.nvm/nvm.sh\" ]; then . \"$HOME/.nvm/nvm.sh\" --no-use; nvm use --silent default >/dev/null 2>&1 || true; fi; exec \"$@\"";

/// Only exact, server-generated launch plans qualify for owned-process cleanup.
#[derive(Clone, Debug)]
pub struct CleanupPlan { runner: PathBuf, distribution: String, token: String }
impl CleanupPlan {
    pub fn from_command(program: &Path, args: &[String]) -> Option<Self> {
        if !program.file_name()?.to_string_lossy().eq_ignore_ascii_case("wsl.exe") || args.len() < 17
            || args[0] != "--distribution" || args[2] != "--cd" || args[4] != "--exec"
            || args[5] != "/bin/bash" || args[6] != "-lc" || args[7] != BOOTSTRAP
            || args[8] != "gptbridge-wsl" || args[9].len() != 36
            || !args[9].bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
            || args[10] != "/usr/bin/setsid" || args[12] != "/usr/bin/timeout" { return None; }
        Some(Self {runner:program.into(),distribution:args[1].clone(),token:args[9].clone()})
    }
    pub fn args(&self) -> Vec<String> {
        vec!["--distribution".into(),self.distribution.clone(),"--exec".into(),
            "/usr/bin/python3".into(),"-c".into(),CLEANUP_SCRIPT.into(),self.token.clone()]
    }
    /// Independent bounded WSL cleanup, before the Windows relay is killed or
    /// permits are released. Never stops a distribution, MCP, or unrelated PID.
    pub fn stop(&self) -> std::io::Result<()> {
        use std::{process::{Command, Stdio},time::{Duration,Instant}};
        let mut cmd=Command::new(&self.runner);
        cmd.args(self.args()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)] {use std::os::windows::process::CommandExt;cmd.creation_flags(0x08000000);}
        let mut child=cmd.spawn()?;
        let start=Instant::now();
        loop {
            if let Some(status)=child.try_wait()? {
                return if status.success(){Ok(())}else{Err(std::io::Error::new(std::io::ErrorKind::Other,"WSL owned-process cleanup failed"))};
            }
            if start.elapsed()>Duration::from_secs(5) {let _=child.kill();let _=child.wait();return Err(std::io::Error::new(std::io::ErrorKind::TimedOut,"WSL cleanup timed out"));}
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Locate only processes carrying this launch's random identity. Recheck before
/// signalling, never infer ownership from process age/name or terminate WSL.
pub const CLEANUP_SCRIPT: &str = r#"import os,sys,signal,time
marker=b'GPTBRIDGE_WSL_EXEC_ID='+sys.argv[1].encode()
def owned(pid):
    try:
        with open('/proc/'+str(pid)+'/environ','rb') as f: return marker in f.read().split(b'\0')
    except (OSError,ValueError): return False
for sig in (signal.SIGTERM,signal.SIGKILL):
    for entry in os.scandir('/proc'):
        if entry.name.isdigit():
            pid=int(entry.name)
            if pid!=os.getpid() and owned(pid):
                try:
                    if owned(pid): os.kill(pid,sig)
                except ProcessLookupError: pass
    if sig==signal.SIGTERM: time.sleep(0.1)
time.sleep(0.02)
if any(e.name.isdigit() and int(e.name)!=os.getpid() and owned(int(e.name)) for e in os.scandir('/proc')): sys.exit(1)
"#;

#[cfg(test)]
mod workspace_mapping_tests {
    use super::*;
    #[test]
    fn unc_aliases_and_canonical_paths() {
        for p in [r"\\wsl.localhost\Ubuntu-24.04\home\li\repo", r"\\wsl$\Ubuntu-24.04\home\li\repo", r"\\?\UNC\wsl.localhost\Ubuntu-24.04\home\li\repo"] {
            let t = WslPath::from_unc(p).unwrap();
            assert_eq!(t.distribution, "Ubuntu-24.04"); assert_eq!(t.linux_path, "/home/li/repo");
        }
        for p in [r"C:\repo", r"\\server\share\repo", r"\\wsl.localhost\..\home", r"\\wsl$\Ubuntu\home\..\outside", r"\\wsl$\Ubuntu:evil\home"] { assert!(WslPath::from_unc(p).is_none(), "{p}"); }
    }
    #[test]
    fn linux_path_scope_and_distribution_identity() {
        let root = WslPath::from_unc(r"\\wsl$\Ubuntu\home\li\repo").unwrap();
        let child = WslPath::from_unc(r"\\wsl.localhost\Ubuntu\home\li\repo\子 目录").unwrap();
        assert!(root.contains(&child));
        for p in [r"\\wsl$\Ubuntu\home\li\repo-other", r"\\wsl$\Debian\home\li\repo", r"\\wsl$\Ubuntu\home\outside"] { assert!(!root.contains(&WslPath::from_unc(p).unwrap())); }
        assert_eq!(root.unc_for_linux("/home/li/.codex/skills/shared/project-workflow.md").unwrap().to_string_lossy(), r"\\wsl$\Ubuntu\home\li\.codex\skills\shared\project-workflow.md");
        for p in ["../escape", "/home/../escape", "//server/path", "/x\\escape", "/x:stream", "/x\0"] { assert!(root.unc_for_linux(p).is_none(), "{p}"); }
    }
    #[test]
    fn positional_argv_preserves_spaces_quotes_and_shell_tokens() {
        let t = WslPath::from_unc(r"\\wsl$\Ubuntu\home\li\有 空格").unwrap();
        let args = vec!["-c".into(), "print('a b')".into(), "$(touch /tmp/never)".into(), "".into(), "a\"b".into()];
        let plan = t.command_args("python3", &args, "12345678-1234-1234-1234-123456789abc", 30000);
        assert_eq!(&plan[..5], ["--distribution", "Ubuntu", "--cd", "/home/li/有 空格", "--exec"]);

        assert_eq!(plan[7], BOOTSTRAP);
        assert_eq!(plan[15], "30.000s");
        assert_eq!(plan[16], "python3");
        assert_eq!(&plan[17..], &args);
    }
    #[test]
    fn skill_roots_match_linux_user_not_windows_home() {
        let t = WslPath::from_unc(r"\\wsl$\Ubuntu\home\li\repo").unwrap();
        let roots = t.skill_roots(); assert_eq!(roots.len(), 2);
        assert!(roots[1].to_string_lossy().ends_with(r"\home\li\.codex\skills"));
        assert!(WslPath::from_unc(r"\\wsl$\Ubuntu\mnt\d\repo").unwrap().skill_roots().is_empty());
        assert!(WslPath::from_unc(r"\\wsl$\Ubuntu\root\repo").unwrap().skill_roots().is_empty());
    }
    #[test]
    fn cleanup_requires_exact_owned_launch_plan() {
        let t=WslPath::from_unc(r"\\wsl$\Ubuntu\home\li\repo").unwrap();
        let mut args=t.command_args("python3",&[],"12345678-1234-1234-1234-123456789abc",1000);
        let plan=CleanupPlan::from_command(Path::new("wsl.exe"),&args).unwrap();
        assert_eq!(plan.args()[1],"Ubuntu");
        assert!(CleanupPlan::from_command(Path::new("other.exe"),&args).is_none());
        args[7]="custom script".into();
        assert!(CleanupPlan::from_command(Path::new("wsl.exe"),&args).is_none());
        args[7]=BOOTSTRAP.into();args[9]="injected; command".into();
        assert!(CleanupPlan::from_command(Path::new("wsl.exe"),&args).is_none());
    }
    #[test]
    fn native_host_read_path_keeps_native_paths() {
        assert_eq!(host_read_path(Path::new("/tmp/repo"), "/tmp/file"), PathBuf::from("/tmp/file"));
        assert_eq!(host_read_path(Path::new(r"C:\repo"), "src/file.rs"), PathBuf::from("src/file.rs"));
    }
}
