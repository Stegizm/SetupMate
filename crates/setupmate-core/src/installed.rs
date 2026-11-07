//! Best-effort, read-only detection of already-installed packages.
//!
//! Debian, Arch, Alpine, and Void databases are read directly as files or
//! directories with no extra dependency. Fedora/openSUSE keep their rpm database
//! in SQLite (`rpmdb.sqlite`), so `rusqlite` is used to read the `Name` index in
//! read-only mode. If the database cannot be found or parsed the snapshot is
//! *unreliable* and every lookup is `Unknown` instead of a guess. That keeps the
//! "never invent" rule intact.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::models::DistroFamily;

const DPKG_STATUS: &str = "/var/lib/dpkg/status";
const PACMAN_LOCAL: &str = "/var/lib/pacman/local";
/// Alpine keeps a plain-text installed database.
const APK_INSTALLED: &str = "/lib/apk/db/installed";
/// Void keeps one XML plist per package under this directory.
const XBPS_DB_DIR: &str = "/var/db/xbps";
/// Fedora/RHEL and openSUSE (Tumbleweed) locations for the SQLite rpm database.
const RPM_DB_PATHS: [&str; 2] = [
    "/var/lib/rpm/rpmdb.sqlite",
    "/usr/lib/sysimage/rpm/rpmdb.sqlite",
];

/// Whether a catalog package appears to be installed locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Installed,
    Missing,
    Unknown,
}

/// A snapshot of locally installed package names for one distro family.
#[derive(Debug, Clone)]
pub struct InstalledPackages {
    names: HashSet<String>,
    reliable: bool,
}

impl InstalledPackages {
    /// Reads the local package database for `family`.
    ///
    /// Families whose database cannot be found or parsed yield an empty,
    /// unreliable snapshot (never a guess).
    #[must_use]
    pub fn detect(family: DistroFamily) -> Self {
        match family {
            DistroFamily::Debian => Self {
                names: read_dpkg_names(),
                // An unreadable dpkg status file also yields an empty snapshot;
                // treat that as reliable-but-empty only when the file existed.
                reliable: fs::metadata(DPKG_STATUS).is_ok(),
            },
            DistroFamily::Arch => Self {
                names: read_pacman_names(),
                reliable: fs::metadata(PACMAN_LOCAL).is_ok(),
            },
            DistroFamily::Fedora | DistroFamily::OpenSuse => match read_rpm_names() {
                Some(names) => Self {
                    names,
                    reliable: true,
                },
                None => Self {
                    names: HashSet::new(),
                    reliable: false,
                },
            },
            DistroFamily::Alpine => Self {
                names: read_apk_names(),
                reliable: fs::metadata(APK_INSTALLED).is_ok(),
            },
            DistroFamily::Void => Self {
                names: read_xbps_names(),
                reliable: fs::metadata(XBPS_DB_DIR).is_ok(),
            },
            DistroFamily::Unknown => Self {
                names: HashSet::new(),
                reliable: false,
            },
        }
    }

    /// Whether the snapshot can be trusted. When `false`, `status` is always
    /// `Unknown`.
    #[must_use]
    pub fn is_reliable(&self) -> bool {
        self.reliable
    }

    /// Number of installed packages recorded in the snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Classifies `package` (the catalog's package name for this family).
    #[must_use]
    pub fn status(&self, package: &str) -> InstallStatus {
        if !self.reliable {
            return InstallStatus::Unknown;
        }
        if self.names.contains(package) {
            InstallStatus::Installed
        } else {
            InstallStatus::Missing
        }
    }
}

fn read_dpkg_names() -> HashSet<String> {
    match fs::read_to_string(DPKG_STATUS) {
        Ok(contents) => parse_dpkg_status(&contents),
        Err(_) => HashSet::new(),
    }
}

/// Parses `/var/lib/dpkg/status`: stanzas separated by blank lines, each holding
/// a `Package:` name and a `Status:` line. Only `install ok installed` counts.
fn parse_dpkg_status(contents: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut current: Option<&str> = None;
    let mut installed = false;

    for line in contents.lines() {
        if line.trim().is_empty() {
            commit_stanza(&mut names, &mut current, &mut installed);
            continue;
        }
        if let Some(value) = line.strip_prefix("Package:") {
            current = Some(value.trim());
        } else if let Some(value) = line.strip_prefix("Status:") {
            installed = value.trim() == "install ok installed";
        }
    }
    commit_stanza(&mut names, &mut current, &mut installed);
    names
}

fn commit_stanza(names: &mut HashSet<String>, current: &mut Option<&str>, installed: &mut bool) {
    if *installed {
        if let Some(name) = current.take() {
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
    }
    *current = None;
    *installed = false;
}

fn read_pacman_names() -> HashSet<String> {
    let entries = match fs::read_dir(PACMAN_LOCAL) {
        Ok(entries) => entries,
        Err(_) => return HashSet::new(),
    };

    let mut names = HashSet::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let dir = file_name.to_string_lossy();
        if let Some(name) = pacman_name_from_dir(&dir) {
            names.insert(name.to_string());
        }
    }
    names
}

/// Arch's local database uses `<pkgname>-<pkgver>-<pkgrel>` directory names, so
/// the package name is everything before the two trailing version segments.
fn pacman_name_from_dir(dir: &str) -> Option<&str> {
    let mut parts = dir.rsplitn(3, '-');
    let _release = parts.next()?;
    let _version = parts.next()?;
    let name = parts.next()?;
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn read_apk_names() -> HashSet<String> {
    match fs::read_to_string(APK_INSTALLED) {
        Ok(contents) => parse_apk_installed(&contents),
        Err(_) => HashSet::new(),
    }
}

/// Parses Alpine's `/lib/apk/db/installed`: one stanza per package with a
/// `P:<name>` line. Every stanza present in that file is installed.
fn parse_apk_installed(contents: &str) -> HashSet<String> {
    contents
        .lines()
        .filter_map(|line| line.strip_prefix("P:"))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn read_xbps_names() -> HashSet<String> {
    let entries = match fs::read_dir(XBPS_DB_DIR) {
        Ok(entries) => entries,
        Err(_) => return HashSet::new(),
    };

    let mut names = HashSet::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !(file_name.starts_with("pkgdb-") && file_name.ends_with(".plist")) {
            continue;
        }
        if let Ok(contents) = fs::read_to_string(entry.path()) {
            names.extend(parse_xbps_plist(&contents));
        }
    }
    names
}

/// Parses an xbps plist. Top-level package keys sit at exactly one indentation
/// level (`\t<key>name-version_rev</key>`); nested property keys are deeper and
/// are ignored.
fn parse_xbps_plist(contents: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in contents.lines() {
        let indentation = line.chars().take_while(|c| *c == '\t').count();
        if indentation != 1 {
            continue;
        }
        let Some(rest) = line.trim_start_matches('\t').strip_prefix("<key>") else {
            continue;
        };
        let Some(key) = rest.strip_suffix("</key>") else {
            continue;
        };
        if let Some(name) = xbps_name_from_key(key) {
            names.insert(name.to_string());
        }
    }
    names
}

/// xbps keys are `<pkgname>-<version>_<revision>`; the version never contains a
/// hyphen because xbps replaces upstream hyphens with underscores.
fn xbps_name_from_key(key: &str) -> Option<&str> {
    let (name, version) = key.rsplit_once('-')?;
    if name.is_empty() || version.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Reads the SQLite rpm database, trying each known location. Returns `None`
/// when no database exists or none could be parsed, so callers report
/// `Unknown` rather than guessing.
fn read_rpm_names() -> Option<HashSet<String>> {
    for path in RPM_DB_PATHS {
        if !Path::new(path).is_file() {
            continue;
        }
        let Ok(connection) =
            rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        if let Ok(names) = rpm_names_from_connection(&connection) {
            return Some(names);
        }
    }
    None
}

/// Reads package names from the rpm `Name` index table. The schema is shared by
/// Fedora/RHEL and openSUSE because both use rpm's SQLite backend.
fn rpm_names_from_connection(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<HashSet<String>> {
    let mut statement = connection.prepare("SELECT key FROM Name")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut names = HashSet::new();
    for row in rows {
        names.insert(row?);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dpkg_status_paragraphs() {
        let contents = "\
Package: curl
Status: install ok installed

Package: htop
Status: install ok installed

Package: removed-thing
Status: deinstall ok config-files

Package: half-installed
Status: install ok half-configured
";
        let names = parse_dpkg_status(contents);
        assert!(names.contains("curl"));
        assert!(names.contains("htop"));
        assert!(!names.contains("removed-thing"));
        assert!(!names.contains("half-installed"));
    }

    #[test]
    fn parses_pacman_directory_names() {
        assert_eq!(pacman_name_from_dir("curl-8.10.1-1"), Some("curl"));
        assert_eq!(
            pacman_name_from_dir("lib32-glibc-2.40+r3+gabc-3"),
            Some("lib32-glibc")
        );
        assert_eq!(pacman_name_from_dir("dangling"), None);
    }

    #[test]
    fn rpm_families_are_unknown_not_missing() {
        // On hosts without an rpm database (e.g. Arch) the snapshot must be
        // unreliable and every lookup Unknown. Skip on rpm-based hosts.
        let has_rpm_db = RPM_DB_PATHS
            .iter()
            .any(|path| std::path::Path::new(path).is_file());
        if has_rpm_db {
            return;
        }
        let snapshot = InstalledPackages::detect(DistroFamily::Fedora);
        assert!(!snapshot.is_reliable());
        assert_eq!(snapshot.status("curl"), InstallStatus::Unknown);
    }

    #[test]
    fn parses_apk_installed_stanzas() {
        let contents = "\
C:Q1abc=
P:curl
V:8.14.1-r0
A:x86_64

C:Q1def=
P:busybox
V:1.37.0-r3
";
        let names = parse_apk_installed(contents);
        assert!(names.contains("curl"));
        assert!(names.contains("busybox"));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn parses_xbps_plist_keys() {
        let contents = "\
<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<plist version=\"1.0\">
<dict>
\t<key>curl-8.14.1_1</key>
\t<dict>
\t\t<key>architecture</key>
\t\t<string>x86_64</string>
\t</dict>
\t<key>lib32-glibc-2.40_1</key>
\t<dict>
\t\t<key>architecture</key>
\t\t<string>x86_64</string>
\t</dict>
</dict>
</plist>
";
        let names = parse_xbps_plist(contents);
        assert!(names.contains("curl"));
        assert!(names.contains("lib32-glibc"));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn strips_xbps_version_suffix() {
        assert_eq!(xbps_name_from_key("curl-8.14.1_1"), Some("curl"));
        assert_eq!(
            xbps_name_from_key("lib32-glibc-2.40_1"),
            Some("lib32-glibc")
        );
        assert_eq!(xbps_name_from_key("dangling"), None);
    }

    #[test]
    fn alpine_and_void_do_not_guess_without_database() {
        if fs::metadata(APK_INSTALLED).is_ok() {
            return;
        }
        let snapshot = InstalledPackages::detect(DistroFamily::Alpine);
        assert!(!snapshot.is_reliable());
        assert_eq!(snapshot.status("curl"), InstallStatus::Unknown);

        if fs::metadata(XBPS_DB_DIR).is_ok() {
            return;
        }
        let snapshot = InstalledPackages::detect(DistroFamily::Void);
        assert!(!snapshot.is_reliable());
        assert_eq!(snapshot.status("curl"), InstallStatus::Unknown);
    }

    #[test]
    fn reads_rpm_name_table() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE Name (key TEXT NOT NULL, hnum INTEGER NOT NULL, idx INTEGER NOT NULL);\
                 INSERT INTO Name VALUES ('curl', 1, 0), ('htop', 2, 0);",
            )
            .unwrap();
        let names = rpm_names_from_connection(&connection).unwrap();
        assert!(names.contains("curl"));
        assert!(names.contains("htop"));
        assert_eq!(names.len(), 2);
    }
}
