//! One argument parser for both policy validation and direct process launch.

pub(super) fn split_command(command: &str) -> Result<Vec<String>, &'static str> {
    #[cfg(windows)]
    {
        split_windows_command(command)
    }
    #[cfg(not(windows))]
    {
        shell_words::split(command).map_err(|_| "Invalid command syntax")
    }
}

/// WSL commands follow POSIX quoting even though the bridge process is Windows.
/// Policy and launch must use the same parser to avoid validating different argv.
pub(super) fn split_for_workspace(command: &str, root: &std::path::Path) -> Result<Vec<String>, &'static str> {
    #[cfg(windows)]
    if super::wsl::for_path(root).is_some() {
        return shell_words::split(command).map_err(|_| "Invalid POSIX command syntax for WSL");
    }
    let _ = root;
    split_command(command)
}

// Windows CRT quoting: backslashes are literal unless immediately before a
// double quote. No shell expansion; single quotes are ordinary characters.
#[cfg(any(windows, test))]
fn split_windows_command(command: &str) -> Result<Vec<String>, &'static str> {
    let mut chars = command.chars().peekable();
    let mut args = Vec::new();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut arg = String::new();
        let mut quoted = false;
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() && !quoted {
                break;
            }
            let mut slashes = 0;
            while chars.peek() == Some(&'\\') {
                chars.next();
                slashes += 1;
            }
            if chars.peek() == Some(&'"') {
                arg.extend(std::iter::repeat_n('\\', slashes / 2));
                chars.next();
                if slashes % 2 == 1 {
                    arg.push('"');
                } else if quoted && chars.peek() == Some(&'"') {
                    chars.next();
                    arg.push('"');
                } else {
                    quoted = !quoted;
                }
            } else {
                arg.extend(std::iter::repeat_n('\\', slashes));
                match chars.peek() {
                    Some(c) if c.is_whitespace() && !quoted => break,
                    Some(_) => arg.push(chars.next().unwrap()),
                    None => break,
                }
            }
        }
        if quoted {
            return Err("Unclosed double quote in Windows command");
        }
        args.push(arg);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn workspace_mapping_tests_wsl_posix_quotes_and_native_windows_quotes_are_distinct() {
        let root=std::path::Path::new(r"\\wsl.localhost\Ubuntu-24.04\home\li\repo");
        assert_eq!(split_for_workspace("python3 -c 'print(\"a b\")'",root).unwrap(),
            ["python3","-c","print(\"a b\")"]);
        assert!(split_for_workspace("python3 'unfinished",root).is_err());
        assert_eq!(split_for_workspace(r#"python -c "print('a b')""#,std::path::Path::new(r"C:\repo")).unwrap(),
            ["python","-c","print('a b')"]);
    }

    #[test]
    fn windows_paths_quotes_and_empty_arguments_round_trip() {
        assert_eq!(
            split_windows_command(
                r#"D:\env\python.exe "D:\project files\test.py" "" C:\plain\path"#
            )
            .unwrap(),
            [
                r"D:\env\python.exe",
                r"D:\project files\test.py",
                "",
                r"C:\plain\path"
            ]
        );
        assert_eq!(
            split_windows_command(
                r#""C:\Program Files\Python\python.exe" -c "print('ok')" "C:\folder\\" "a\"b""#
            )
            .unwrap(),
            [
                r"C:\Program Files\Python\python.exe",
                "-c",
                "print('ok')",
                "C:\\folder\\",
                "a\"b"
            ]
        );
        assert!(split_windows_command("python \"unfinished").is_err());
    }
}
