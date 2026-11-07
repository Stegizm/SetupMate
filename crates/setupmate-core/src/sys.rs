//! Small read-only system helpers shared by the executor and the updater.
//!
//! Nothing here modifies the system; these are presence checks and a root check.

use std::env;
use std::path::{Path, PathBuf};

/// Looks up an executable in `PATH` without running anything.
#[must_use]
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Whether the current process runs as root, read from `/proc/self/status`.
///
/// Missing or unreadable `/proc` degrades to `false`; the caller then falls back
/// to an elevation helper instead of assuming privileges.
#[must_use]
pub fn is_root() -> bool {
    let Ok(contents) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    contents
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|value| value.split_whitespace().next())
        .is_some_and(|uid| uid == "0")
}

/// Returns the prefix needed to run a privileged command, if available.
///
/// - root: no prefix
/// - otherwise `pkexec`, when installed
/// - otherwise an explanation the GUI can show verbatim
pub fn elevation_prefix() -> Result<Vec<String>, String> {
    if is_root() {
        return Ok(Vec::new());
    }
    if find_in_path("pkexec").is_some() {
        return Ok(vec!["pkexec".to_string()]);
    }
    Err(
        "Yönetici yetkisi için pkexec bulunamadı. Komutları terminalde `sudo` ile \
         çalıştırabilirsin."
            .to_string(),
    )
}

/// Whether `path` points to an executable file.
#[must_use]
pub fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_known_executable() {
        // `sh` exists on every supported Linux system.
        assert!(find_in_path("sh").is_some());
        assert!(find_in_path("definitely-not-a-real-binary-xyz").is_none());
    }

    #[test]
    fn root_check_does_not_panic() {
        let _ = is_root();
    }
}
