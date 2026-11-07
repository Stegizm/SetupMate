//! Package manager mapping and command construction.
//!
//! This module only builds command argument vectors; it never executes anything.

use crate::models::{DistroFamily, PackageManagerKind};

/// Maps a detected distribution family to its package manager.
#[must_use]
pub fn manager_for_family(family: DistroFamily) -> PackageManagerKind {
    match family {
        DistroFamily::Debian => PackageManagerKind::Apt,
        DistroFamily::Fedora => PackageManagerKind::Dnf,
        DistroFamily::Arch => PackageManagerKind::Pacman,
        DistroFamily::OpenSuse => PackageManagerKind::Zypper,
        DistroFamily::Alpine => PackageManagerKind::Apk,
        DistroFamily::Void => PackageManagerKind::Xbps,
        DistroFamily::Unknown => PackageManagerKind::Unknown,
    }
}

/// Returns the package manager executable name, if known.
#[must_use]
pub fn executable(kind: PackageManagerKind) -> Option<&'static str> {
    match kind {
        PackageManagerKind::Apt => Some("apt-get"),
        PackageManagerKind::Dnf => Some("dnf"),
        PackageManagerKind::Pacman => Some("pacman"),
        PackageManagerKind::Zypper => Some("zypper"),
        PackageManagerKind::Apk => Some("apk"),
        PackageManagerKind::Xbps => Some("xbps-install"),
        PackageManagerKind::Unknown => None,
    }
}

/// Builds an install command for one package.
///
/// Returns `None` for unknown managers. Callers must only pass verified package
/// names; `TODO_VERIFY` entries must never reach this function.
#[must_use]
pub fn install_command(kind: PackageManagerKind, package: &str) -> Option<Vec<String>> {
    let executable = executable(kind)?;
    let mut command = vec![executable.to_string()];

    match kind {
        PackageManagerKind::Apt => {
            command.push("install".to_string());
            command.push("--no-install-recommends".to_string());
        }
        PackageManagerKind::Dnf => command.push("install".to_string()),
        PackageManagerKind::Pacman => command.push("-S".to_string()),
        PackageManagerKind::Zypper => command.push("install".to_string()),
        PackageManagerKind::Apk => command.push("add".to_string()),
        PackageManagerKind::Xbps => {
            command.push("-S".to_string());
        }
        PackageManagerKind::Unknown => return None,
    }

    command.push(package.to_string());
    Some(command)
}

/// Flag that makes a package manager accept the install without prompting.
#[must_use]
pub fn non_interactive_flag(kind: PackageManagerKind) -> Option<&'static str> {
    match kind {
        PackageManagerKind::Apt => Some("-y"),
        PackageManagerKind::Dnf => Some("-y"),
        PackageManagerKind::Pacman => Some("--noconfirm"),
        PackageManagerKind::Zypper => Some("--non-interactive"),
        PackageManagerKind::Apk => Some("--no-interactive"),
        PackageManagerKind::Xbps => Some("-y"),
        PackageManagerKind::Unknown => None,
    }
}

/// Builds a non-interactive install command used by the live (approved) path.
///
/// The plan shown to the user keeps the plain command; only the executed
/// command gets the confirmation flag. `TODO_VERIFY` entries never reach here.
#[must_use]
pub fn non_interactive_command(kind: PackageManagerKind, package: &str) -> Option<Vec<String>> {
    let mut command = install_command(kind, package)?;
    let flag = non_interactive_flag(kind)?;
    // executable + subcommand + flag(s) + package: insert right after the
    // subcommand so the flag is accepted by every supported manager.
    let position = command.len().saturating_sub(1).min(2);
    command.insert(position, flag.to_string());
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_families_to_managers() {
        assert_eq!(
            manager_for_family(DistroFamily::Debian),
            PackageManagerKind::Apt
        );
        assert_eq!(
            manager_for_family(DistroFamily::Fedora),
            PackageManagerKind::Dnf
        );
        assert_eq!(
            manager_for_family(DistroFamily::Arch),
            PackageManagerKind::Pacman
        );
        assert_eq!(
            manager_for_family(DistroFamily::OpenSuse),
            PackageManagerKind::Zypper
        );
        assert_eq!(
            manager_for_family(DistroFamily::Alpine),
            PackageManagerKind::Apk
        );
        assert_eq!(
            manager_for_family(DistroFamily::Void),
            PackageManagerKind::Xbps
        );
        assert_eq!(
            manager_for_family(DistroFamily::Unknown),
            PackageManagerKind::Unknown
        );
    }

    #[test]
    fn builds_known_install_commands() {
        assert_eq!(
            install_command(PackageManagerKind::Apt, "example"),
            Some(vec![
                "apt-get".to_string(),
                "install".to_string(),
                "--no-install-recommends".to_string(),
                "example".to_string(),
            ])
        );
        assert_eq!(
            install_command(PackageManagerKind::Pacman, "example"),
            Some(vec![
                "pacman".to_string(),
                "-S".to_string(),
                "example".to_string(),
            ])
        );
        assert_eq!(
            install_command(PackageManagerKind::Apk, "example"),
            Some(vec![
                "apk".to_string(),
                "add".to_string(),
                "example".to_string(),
            ])
        );
        assert_eq!(
            install_command(PackageManagerKind::Xbps, "example"),
            Some(vec![
                "xbps-install".to_string(),
                "-S".to_string(),
                "example".to_string(),
            ])
        );
    }

    #[test]
    fn builds_non_interactive_commands() {
        assert_eq!(
            non_interactive_command(PackageManagerKind::Apt, "example"),
            Some(vec![
                "apt-get".to_string(),
                "install".to_string(),
                "-y".to_string(),
                "--no-install-recommends".to_string(),
                "example".to_string(),
            ])
        );
        assert_eq!(
            non_interactive_command(PackageManagerKind::Dnf, "example"),
            Some(vec![
                "dnf".to_string(),
                "install".to_string(),
                "-y".to_string(),
                "example".to_string(),
            ])
        );
        assert_eq!(
            non_interactive_command(PackageManagerKind::Pacman, "example"),
            Some(vec![
                "pacman".to_string(),
                "-S".to_string(),
                "--noconfirm".to_string(),
                "example".to_string(),
            ])
        );
        assert_eq!(
            non_interactive_command(PackageManagerKind::Zypper, "example"),
            Some(vec![
                "zypper".to_string(),
                "install".to_string(),
                "--non-interactive".to_string(),
                "example".to_string(),
            ])
        );
        assert_eq!(
            non_interactive_command(PackageManagerKind::Apk, "example"),
            Some(vec![
                "apk".to_string(),
                "add".to_string(),
                "--no-interactive".to_string(),
                "example".to_string(),
            ])
        );
        assert_eq!(
            non_interactive_command(PackageManagerKind::Xbps, "example"),
            Some(vec![
                "xbps-install".to_string(),
                "-S".to_string(),
                "-y".to_string(),
                "example".to_string(),
            ])
        );
    }

    #[test]
    fn unknown_manager_has_no_command() {
        assert_eq!(
            install_command(PackageManagerKind::Unknown, "example"),
            None
        );
        assert_eq!(executable(PackageManagerKind::Unknown), None);
        assert_eq!(
            non_interactive_command(PackageManagerKind::Unknown, "example"),
            None
        );
    }
}
