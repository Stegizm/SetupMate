//! Self-update support for the AppImage build.
//!
//! The update metadata comes from the GitHub release's `SHA256SUMS` asset, so no
//! JSON parsing or extra dependency is needed. Downloads use `curl` or `wget`
//! and are verified with the in-tree SHA-256 implementation before replacing the
//! running AppImage. AUR/package-manager installs are intentionally **not**
//! self-updated; those users update with their package manager.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::sha256::Sha256;
use crate::sys::find_in_path;

/// Default GitHub repository slug. Override at runtime with `SETUPMATE_REPO`.
pub const DEFAULT_REPO: &str = "Stegizm/SetupMate";
/// Environment variable that overrides the repository slug.
pub const REPO_ENV: &str = "SETUPMATE_REPO";
/// Environment variable AppImage sets to the path of the running image.
const APPIMAGE_ENV: &str = "APPIMAGE";

/// Update failure. Messages are user-facing Turkish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    NoDownloader,
    Download(String),
    InvalidMetadata(String),
    NotAppImage,
    Io(String),
    ChecksumMismatch { expected: String, actual: String },
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDownloader => write!(
                f,
                "Güncelleme için `curl` veya `wget` gerekli; ikisi de bulunamadı."
            ),
            Self::Download(message) => write!(f, "İndirme başarısız: {message}"),
            Self::InvalidMetadata(message) => {
                write!(f, "Güncelleme bilgisi okunamadı: {message}")
            }
            Self::NotAppImage => write!(
                f,
                "Bu kurulum AppImage değil. AUR/paket yöneticisi kurulumunu paket \
                 yöneticinle güncelle (ör. `yay -Syu`)."
            ),
            Self::Io(message) => write!(f, "Dosya işlemi başarısız: {message}"),
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "İndirilen dosyanın özeti eşleşmedi (beklenen {expected}, bulunan {actual})."
            ),
        }
    }
}

impl std::error::Error for UpdateError {}

/// A newer release found on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub asset_name: String,
    pub asset_url: String,
    /// Expected SHA-256 from `SHA256SUMS`, lowercase hex.
    pub sha256: String,
}

/// Result of an update attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate {
        current: String,
    },
    Updated {
        from: String,
        to: String,
        path: PathBuf,
    },
}

/// Repository slug: `SETUPMATE_REPO` when set, otherwise [`DEFAULT_REPO`].
#[must_use]
pub fn repo_slug() -> String {
    env::var(REPO_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_REPO.to_string())
}

/// Picks the checksum line for `SetupMate-<version>-<arch>.AppImage`.
///
/// Returns `(file_name, sha256_hex)`. Unsafe names (slashes, `..`) are rejected.
#[must_use]
pub fn select_asset(checksums: &str, arch: &str) -> Option<(String, String)> {
    let suffix = format!("-{arch}.AppImage");
    checksums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if !is_safe_asset_name(name) || !name.starts_with("SetupMate-") || !name.ends_with(&suffix)
        {
            return None;
        }
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        Some((name.to_string(), hash.to_ascii_lowercase()))
    })
}

/// Extracts `0.1.0` from `SetupMate-0.1.0-x86_64.AppImage`.
#[must_use]
pub fn version_from_asset(name: &str) -> Option<String> {
    let stem = name.strip_prefix("SetupMate-")?.strip_suffix(".AppImage")?;
    let (version, _arch) = stem.rsplit_once('-')?;
    if version.is_empty() || version.contains('-') {
        return None;
    }
    Some(version.to_string())
}

fn is_safe_asset_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Numeric semantic-version comparison. Pre-release suffixes are ignored.
#[must_use]
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (version_parts(latest), version_parts(current)) {
        (Some(latest_parts), Some(current_parts)) => latest_parts > current_parts,
        _ => latest != current,
    }
}

fn version_parts(version: &str) -> Option<Vec<u64>> {
    let trimmed = version.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for piece in trimmed.split('.') {
        let digits: String = piece.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return None;
        }
        parts.push(digits.parse().ok()?);
    }
    Some(parts)
}

/// Checks GitHub for a newer AppImage release.
pub fn check_for_update(
    repo: &str,
    current_version: &str,
    arch: &str,
) -> Result<Option<UpdateInfo>, UpdateError> {
    let base = format!("https://github.com/{repo}/releases/latest/download");
    let checksums = download_text(&format!("{base}/SHA256SUMS"))?;
    let (asset_name, sha256) = select_asset(&checksums, arch).ok_or_else(|| {
        UpdateError::InvalidMetadata(format!("{arch} için AppImage varlığı bulunamadı"))
    })?;
    let latest = version_from_asset(&asset_name)
        .ok_or_else(|| UpdateError::InvalidMetadata(format!("sürüm okunamadı: {asset_name}")))?;

    if !is_newer(&latest, current_version) {
        return Ok(None);
    }

    Ok(Some(UpdateInfo {
        current: current_version.to_string(),
        latest,
        asset_url: format!("{base}/{asset_name}"),
        asset_name,
        sha256,
    }))
}

/// Checks for an update and, when available, replaces the running AppImage.
///
/// `target` overrides the destination path; by default `$APPIMAGE` is used.
pub fn update(
    repo: &str,
    current_version: &str,
    arch: &str,
    target: Option<PathBuf>,
) -> Result<UpdateOutcome, UpdateError> {
    // Resolve the destination first: a package-manager install must report
    // `NotAppImage` without making a network request.
    let path = match target {
        Some(path) => path,
        None => env::var_os(APPIMAGE_ENV)
            .map(PathBuf::from)
            .ok_or(UpdateError::NotAppImage)?,
    };

    let Some(info) = check_for_update(repo, current_version, arch)? else {
        return Ok(UpdateOutcome::UpToDate {
            current: current_version.to_string(),
        });
    };

    replace_appimage(&info.asset_url, &info.sha256, &path)?;

    Ok(UpdateOutcome::Updated {
        from: info.current,
        to: info.latest,
        path,
    })
}

fn replace_appimage(url: &str, expected: &str, target: &Path) -> Result<(), UpdateError> {
    let directory = target.parent().ok_or_else(|| {
        UpdateError::Io(format!("hedef dizin belirlenemedi: {}", target.display()))
    })?;
    let temp = directory.join(format!(
        ".{}.new",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("setupmate.AppImage")
    ));

    download_to_file(url, &temp)?;

    let actual = hash_file(&temp)?;
    if actual != expected {
        let _ = fs::remove_file(&temp);
        return Err(UpdateError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }

    set_executable(&temp)?;
    fs::rename(&temp, target)
        .map_err(|error| UpdateError::Io(format!("{}: {error}", target.display())))?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, UpdateError> {
    let mut file = fs::File::open(path)
        .map_err(|error| UpdateError::Io(format!("{}: {error}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| UpdateError::Io(format!("{}: {error}", path.display())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finish_hex())
}

fn set_executable(path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| UpdateError::Io(format!("{}: {error}", path.display())))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|error| UpdateError::Io(format!("{}: {error}", path.display())))
}

fn download_text(url: &str) -> Result<String, UpdateError> {
    if find_in_path("curl").is_some() {
        let output = Command::new("curl")
            .args(["-fsSL", "--connect-timeout", "15", "--max-time", "120", url])
            .output()
            .map_err(|error| UpdateError::Download(error.to_string()))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        return Err(UpdateError::Download(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    if find_in_path("wget").is_some() {
        let output = Command::new("wget")
            .args(["-qO-", "--timeout=30", url])
            .output()
            .map_err(|error| UpdateError::Download(error.to_string()))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        return Err(UpdateError::Download(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Err(UpdateError::NoDownloader)
}

fn download_to_file(url: &str, path: &Path) -> Result<(), UpdateError> {
    if find_in_path("curl").is_some() {
        let output = Command::new("curl")
            .args(["-fL", "--connect-timeout", "15", "--max-time", "600", "-o"])
            .arg(path)
            .arg(url)
            .output()
            .map_err(|error| UpdateError::Download(error.to_string()))?;
        if output.status.success() {
            return Ok(());
        }
        return Err(UpdateError::Download(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    if find_in_path("wget").is_some() {
        let output = Command::new("wget")
            .args(["-q", "--timeout=60", "-O"])
            .arg(path)
            .arg(url)
            .output()
            .map_err(|error| UpdateError::Download(error.to_string()))?;
        if output.status.success() {
            return Ok(());
        }
        return Err(UpdateError::Download(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Err(UpdateError::NoDownloader)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_matching_asset() {
        let second = "b".repeat(64);
        let third = "c".repeat(64);
        let checksums = format!(
            "\n{second}  SetupMate-0.1.0-x86_64.AppImage\n{third}  SetupMate-0.1.0-aarch64.AppImage\n"
        );
        let (name, hash) = select_asset(&checksums, "x86_64").unwrap();
        assert_eq!(name, "SetupMate-0.1.0-x86_64.AppImage");
        assert_eq!(hash, second);
    }

    #[test]
    fn rejects_path_traversal_assets() {
        let checksums = format!("{}  ../SetupMate-0.1.0-x86_64.AppImage\n", "a".repeat(64));
        assert!(select_asset(&checksums, "x86_64").is_none());
    }

    #[test]
    fn extracts_versions_from_asset_names() {
        assert_eq!(
            version_from_asset("SetupMate-0.1.0-x86_64.AppImage").as_deref(),
            Some("0.1.0")
        );
        assert_eq!(version_from_asset("SetupMate-x86_64.AppImage"), None);
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(is_newer("v1.0.0", "0.9.0"));
    }
}
