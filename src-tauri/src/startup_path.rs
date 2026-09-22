//! Process-local tool discovery for macOS GUI launches.
//! No shell profiles, system PATH edits, permission grants or tool installation.

#[cfg(any(target_os = "macos", test))]
use std::{env, ffi::{OsStr, OsString}, path::{Path, PathBuf}};

/// Preserve configured precedence; only append installed absolute directories.
/// Pure with respect to the environment, so tests never mutate process PATH.
#[cfg(any(target_os = "macos", test))]
fn augmented_path(base: Option<&OsStr>, candidates: &[PathBuf]) -> Option<OsString> {
    let fallback = OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin");
    let base = base.filter(|value| !value.is_empty()).unwrap_or(fallback);
    let mut entries: Vec<PathBuf> = env::split_paths(base).collect();
    for candidate in candidates {
        if candidate.is_absolute() && candidate.is_dir() && !entries.contains(candidate) {
            entries.push(candidate.clone());
        }
    }
    env::join_paths(entries).ok()
}

/// Call exactly once, as the first statement of the application main function.
pub fn bootstrap() {
    #[cfg(target_os = "macos")]
    {
        let mut candidates = vec![PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")];
        if let Some(home) = env::var_os("HOME").filter(|home| Path::new(home).is_absolute()) {
            candidates.push(PathBuf::from(home).join(".cargo/bin"));
        }
        if let Some(path) = augmented_path(env::var_os("PATH").as_deref(), &candidates) {
            // SAFETY: main calls this before creating threads or entering any
            // runtime. Never call from a tool handler or an environment test.
            // Existing allowlists and canonical executable checks are unchanged.
            unsafe { env::set_var("PATH", path); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(value: &OsStr) -> Vec<PathBuf> { env::split_paths(value).collect() }
    fn present_dir() -> PathBuf { env::current_dir().unwrap().canonicalize().unwrap() }

    #[test]
    fn appends_installed_directory_after_explicit_path() {
        let directory = present_dir();
        let base = env::join_paths([PathBuf::from("/explicit/toolchain"), PathBuf::from("/usr/bin")]).unwrap();
        let result = augmented_path(Some(&base), &[directory.clone()]).unwrap();
        assert_eq!(entries(&result), vec![PathBuf::from("/explicit/toolchain"), PathBuf::from("/usr/bin"), directory]);
    }

    #[test]
    fn does_not_append_duplicates() {
        let directory = present_dir();
        let base = env::join_paths([&directory]).unwrap();
        assert_eq!(augmented_path(Some(&base), &[directory.clone(), directory]).unwrap(), base);
    }

    #[test]
    fn absent_and_empty_path_have_system_defaults() {
        let default = OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin");
        assert_eq!(augmented_path(None, &[]).unwrap(), default);
        assert_eq!(augmented_path(Some(OsStr::new("")), &[]).unwrap(), default);
    }

    #[test]
    fn never_adds_relative_or_missing_candidates() {
        let base = OsStr::new("/usr/bin:/bin");
        let missing = present_dir().join(format!(".missing-gm2-startup-{}", std::process::id()));
        assert!(!missing.exists());
        assert_eq!(augmented_path(Some(base), &[PathBuf::from("."), missing]).unwrap(), base);
    }

    #[test]
    fn never_adds_files_as_directories() {
        let base = OsStr::new("/usr/bin:/bin");
        let executable = env::current_exe().unwrap();
        assert_eq!(augmented_path(Some(base), &[executable]).unwrap(), base);
    }

    #[test]
    fn repeated_augmentation_is_idempotent() {
        let candidates = vec![present_dir()];
        let first = augmented_path(Some(OsStr::new("/usr/bin")), &candidates).unwrap();
        assert_eq!(augmented_path(Some(&first), &candidates).unwrap(), first);
    }

    #[cfg(unix)]
    #[test]
    fn preserves_non_utf8_and_spaces_in_configured_entries() {
        use std::os::unix::ffi::OsStringExt;
        let base = OsString::from_vec(b"/explicit toolchain/\xff:/usr/bin".to_vec());
        assert_eq!(augmented_path(Some(&base), &[]).unwrap(), base);
    }

    #[test]
    fn explicit_empty_component_is_not_created_or_reordered() {
        let base = OsStr::new("/explicit/bin::/usr/bin");
        assert_eq!(augmented_path(Some(base), &[]).unwrap(), base);
    }
}
