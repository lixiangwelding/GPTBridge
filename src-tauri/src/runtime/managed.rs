/// Recognize the personal MCP listener owned by this workspace's launchd job.
/// A reachable port or a matching executable alone is not enough to claim it.
pub fn managed_personal_mcp_pid(profile_id: &str, port: u16) -> Option<u32> {
    #[cfg(target_os = "macos")]
    {
        use std::path::Path;
        use std::process::Command;

        if profile_id.is_empty()
            || !profile_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return None;
        }

        let pid = crate::platform::platform()
            .find_pid_listening_on_port(port)
            .ok()??;
        if crate::runtime::is_own_process(pid) {
            return None;
        }

        let current_exe = std::env::current_exe().ok()?.canonicalize().ok()?;
        let process_image = crate::platform::platform().process_image_path(pid).ok()??;
        if Path::new(&process_image).canonicalize().ok()? != current_exe {
            return None;
        }

        let label = format!("com.lixiangwelding.codingtools.personal.{profile_id}");
        let plist = dirs::home_dir()?
            .join("Library/LaunchAgents")
            .join(format!("{label}.plist"));
        if plist.is_symlink() || !plist.is_file() {
            return None;
        }

        let target = format!("gui/{}/{label}", unsafe { libc::getuid() });
        let launchctl = Command::new("/bin/launchctl")
            .args(["print", &target])
            .output()
            .ok()?;
        if !launchctl.status.success()
            || !launchd_job_matches(
                &String::from_utf8(launchctl.stdout).ok()?,
                &current_exe.to_string_lossy(),
                &plist.to_string_lossy(),
                profile_id,
                port,
                pid,
            )
        {
            return None;
        }

        let process = Command::new("/bin/ps")
            .args(["-ww", "-p", &pid.to_string(), "-o", "command="])
            .output()
            .ok()?;
        let expected = format!(
            "{} --personal-serve {profile_id} {port}",
            current_exe.display()
        );
        if !process.status.success() || String::from_utf8(process.stdout).ok()?.trim() != expected {
            return None;
        }

        Some(pid)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (profile_id, port);
        None
    }
}

#[cfg(any(target_os = "macos", test))]
fn launchd_job_matches(
    output: &str,
    executable: &str,
    plist: &str,
    profile_id: &str,
    port: u16,
    pid: u32,
) -> bool {
    let lines: Vec<&str> = output.lines().map(str::trim).collect();
    let expected_pid = format!("pid = {pid}");
    let expected_program = format!("program = {executable}");
    let expected_plist = format!("path = {plist}");
    if !lines.contains(&"state = running")
        || !lines.contains(&expected_pid.as_str())
        || !lines.contains(&expected_program.as_str())
        || !lines.contains(&expected_plist.as_str())
    {
        return false;
    }

    let Some((_, arguments)) = output.split_once("arguments = {") else {
        return false;
    };
    let Some((arguments, _)) = arguments.split_once('}') else {
        return false;
    };
    let actual: Vec<&str> = arguments
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let expected_port = port.to_string();
    actual
        == [
            executable,
            "--personal-serve",
            profile_id,
            expected_port.as_str(),
        ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXECUTABLE: &str = "/Applications/GPTBridge.app/Contents/MacOS/coding-tools-mcp-personal";
    const PLIST: &str =
        "/Users/test/Library/LaunchAgents/com.lixiangwelding.codingtools.personal.profile-a.plist";

    fn job(pid: u32, profile_id: &str, port: u16) -> String {
        format!(
            "path = {PLIST}\nstate = running\nprogram = {EXECUTABLE}\narguments = {{\n  {EXECUTABLE}\n  --personal-serve\n  {profile_id}\n  {port}\n}}\npid = {pid}\n"
        )
    }

    #[test]
    fn only_accepts_the_exact_workspace_job_and_listener_pid() {
        let output = job(42, "profile-a", 28766);
        assert!(launchd_job_matches(
            &output,
            EXECUTABLE,
            PLIST,
            "profile-a",
            28766,
            42
        ));
        assert!(!launchd_job_matches(
            &output,
            EXECUTABLE,
            PLIST,
            "profile-b",
            28766,
            42
        ));
        assert!(!launchd_job_matches(
            &output,
            EXECUTABLE,
            PLIST,
            "profile-a",
            28767,
            42
        ));
        assert!(!launchd_job_matches(
            &output,
            EXECUTABLE,
            PLIST,
            "profile-a",
            28766,
            43
        ));
        assert!(!launchd_job_matches(
            &output,
            "/other/app",
            PLIST,
            "profile-a",
            28766,
            42
        ));
        assert!(!launchd_job_matches(
            &output,
            EXECUTABLE,
            "/other/job.plist",
            "profile-a",
            28766,
            42
        ));
        assert!(!launchd_job_matches(
            &output.replace("--personal-serve", "--personal-job-worker"),
            EXECUTABLE,
            PLIST,
            "profile-a",
            28766,
            42
        ));
        assert!(!launchd_job_matches(
            &output.replace("state = running", "state = waiting"),
            EXECUTABLE,
            PLIST,
            "profile-a",
            28766,
            42
        ));
    }
}
