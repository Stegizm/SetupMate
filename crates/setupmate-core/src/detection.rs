//! Read-only system detection.
//!
//! All detection here reads local files or environment variables only; it never
//! runs external commands or modifies the system. Missing or unreadable sources
//! degrade to `Unknown` instead of failing.

use std::env;
use std::fs;

use crate::models::{DistroFamily, FormFactor, GpuVendor, SessionType, SystemInfo};

/// Detects system facts from read-only sources.
#[must_use]
pub fn detect() -> SystemInfo {
    SystemInfo {
        distro_family: detect_distro_family(),
        gpu_vendors: detect_gpu_vendors(),
        form_factor: detect_form_factor(),
        session_type: detect_session_type(),
    }
}

fn detect_distro_family() -> DistroFamily {
    match read_first_existing(&["/etc/os-release", "/usr/lib/os-release"]) {
        Some(contents) => distro_family_from_os_release(&contents),
        None => DistroFamily::Unknown,
    }
}

fn detect_session_type() -> SessionType {
    session_type_from_env(env::var("XDG_SESSION_TYPE").ok().as_deref())
}

fn detect_form_factor() -> FormFactor {
    let raw = fs::read_to_string("/sys/class/dmi/id/chassis_type").ok();
    form_factor_from_chassis_type(raw.as_deref().map(str::trim))
}

/// Collects every distinct GPU vendor exposed by `/sys/class/drm`.
///
/// Hybrid machines (e.g. Intel iGPU + NVIDIA dGPU) are both reported, so no
/// single GPU is silently dropped based on directory ordering. Unknown or
/// unreadable entries are skipped.
fn detect_gpu_vendors() -> Vec<GpuVendor> {
    let entries = match fs::read_dir("/sys/class/drm") {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut vendors: Vec<GpuVendor> = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if !is_drm_card_name(&name) {
            continue;
        }

        let vendor_path = entry.path().join("device").join("vendor");
        let raw = match fs::read_to_string(vendor_path) {
            Ok(raw) => raw,
            Err(_) => continue,
        };

        let vendor = gpu_vendor_from_pci_id(Some(raw.trim()));
        if vendor != GpuVendor::Unknown && !vendors.contains(&vendor) {
            vendors.push(vendor);
        }
    }

    vendors
}

fn read_first_existing(paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| fs::read_to_string(path).ok())
}

fn distro_family_from_os_release(contents: &str) -> DistroFamily {
    let mut tokens = Vec::new();
    for key in ["ID", "ID_LIKE"] {
        if let Some(value) = os_release_value(contents, key) {
            for token in value.split_whitespace() {
                tokens.push(token.to_ascii_lowercase());
            }
        }
    }

    for token in &tokens {
        match token.as_str() {
            // Debian and its derivatives (Mint, Pop!_OS, Kali, ...). `ID_LIKE`
            // usually already names `debian`/`ubuntu`; the extra spellings keep
            // detection working on images that only set `ID`.
            "debian" | "ubuntu" | "linuxmint" | "mint" | "lmde" | "pop" | "pop_os"
            | "elementary" | "zorin" | "kali" | "raspbian" | "devuan" | "neon" | "mx"
            | "deepin" | "parrot" => return DistroFamily::Debian,
            // Fedora and rpm-based derivatives that use dnf.
            "fedora" | "rhel" | "centos" | "almalinux" | "rocky" | "ol" | "nobara" | "amzn" => {
                return DistroFamily::Fedora
            }
            // Arch and Arch-based derivatives. CachyOS/Manjaro/EndeavourOS all
            // set `ID_LIKE=arch`, but `ID` alone is covered too.
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "garuda" | "artix" | "arcolinux"
            | "archcraft" => return DistroFamily::Arch,
            // openSUSE variants (Leap/Tumbleweed/MicroOS) and SUSE Enterprise.
            "opensuse"
            | "opensuse-leap"
            | "opensuse-tumbleweed"
            | "opensuse-microos"
            | "suse"
            | "sles"
            | "sled" => return DistroFamily::OpenSuse,
            // Alpine and Alpine-based (postmarketOS).
            "alpine" | "postmarketos" => return DistroFamily::Alpine,
            "void" | "voidlinux" => return DistroFamily::Void,
            _ => {}
        }
    }

    DistroFamily::Unknown
}

fn os_release_value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (candidate, value) = line.split_once('=')?;
        if candidate.trim() != key {
            return None;
        }
        Some(value.trim().trim_matches('"').trim_matches('\''))
    })
}

fn session_type_from_env(value: Option<&str>) -> SessionType {
    match value {
        Some(value) if value.eq_ignore_ascii_case("wayland") => SessionType::Wayland,
        Some(value) if value.eq_ignore_ascii_case("x11") => SessionType::X11,
        _ => SessionType::Unknown,
    }
}

fn form_factor_from_chassis_type(value: Option<&str>) -> FormFactor {
    match value.and_then(|value| value.parse::<u8>().ok()) {
        Some(8 | 9 | 10 | 14) => FormFactor::Laptop,
        Some(3 | 4 | 5 | 6 | 7 | 13 | 15 | 16) => FormFactor::Desktop,
        _ => FormFactor::Unknown,
    }
}

fn gpu_vendor_from_pci_id(value: Option<&str>) -> GpuVendor {
    match value.map(str::to_ascii_lowercase).as_deref() {
        Some("0x10de") => GpuVendor::Nvidia,
        Some("0x1002" | "0x1022") => GpuVendor::Amd,
        Some("0x8086") => GpuVendor::Intel,
        _ => GpuVendor::Unknown,
    }
}

fn is_drm_card_name(name: &str) -> bool {
    name.strip_prefix("card")
        .map(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_debian_family_from_id() {
        assert_eq!(
            distro_family_from_os_release("ID=ubuntu\nID_LIKE=\"debian\"\n"),
            DistroFamily::Debian
        );
    }

    #[test]
    fn detects_arch_family_from_id_like() {
        assert_eq!(
            distro_family_from_os_release("ID=manjaro\nID_LIKE=arch\n"),
            DistroFamily::Arch
        );
    }

    #[test]
    fn detects_fedora_family() {
        assert_eq!(
            distro_family_from_os_release("ID=\"rhel\"\n"),
            DistroFamily::Fedora
        );
    }

    #[test]
    fn detects_debian_derivatives() {
        for id in [
            "linuxmint",
            "pop",
            "elementary",
            "zorin",
            "kali",
            "raspbian",
            "devuan",
        ] {
            assert_eq!(
                distro_family_from_os_release(&format!("ID={id}\n")),
                DistroFamily::Debian,
                "failed for {id}"
            );
        }
    }

    #[test]
    fn detects_fedora_derivatives() {
        for (id, id_like) in [
            ("nobara", "fedora"),
            ("almalinux", "rhel centos fedora"),
            ("rocky", "rhel centos fedora"),
            ("ol", "fedora"),
        ] {
            assert_eq!(
                distro_family_from_os_release(&format!("ID={id}\nID_LIKE=\"{id_like}\"\n")),
                DistroFamily::Fedora,
                "failed for {id}"
            );
        }
    }

    #[test]
    fn detects_arch_derivatives() {
        for id in ["manjaro", "endeavouros", "cachyos", "garuda", "artix"] {
            assert_eq!(
                distro_family_from_os_release(&format!("ID={id}\nID_LIKE=arch\n")),
                DistroFamily::Arch,
                "failed for {id}"
            );
        }
    }

    #[test]
    fn detects_opensuse_variants() {
        for id in [
            "opensuse-leap",
            "opensuse-tumbleweed",
            "opensuse-microos",
            "sles",
        ] {
            assert_eq!(
                distro_family_from_os_release(&format!("ID={id}\n")),
                DistroFamily::OpenSuse,
                "failed for {id}"
            );
        }
    }

    #[test]
    fn detects_alpine_and_void() {
        assert_eq!(
            distro_family_from_os_release("ID=alpine\n"),
            DistroFamily::Alpine
        );
        assert_eq!(
            distro_family_from_os_release("ID=postmarketos\nID_LIKE=alpine\n"),
            DistroFamily::Alpine
        );
        assert_eq!(
            distro_family_from_os_release("ID=void\n"),
            DistroFamily::Void
        );
    }

    #[test]
    fn unknown_os_release_is_unknown() {
        assert_eq!(
            distro_family_from_os_release("ID=mystery\n"),
            DistroFamily::Unknown
        );
    }

    #[test]
    fn maps_chassis_types() {
        assert_eq!(form_factor_from_chassis_type(Some("9")), FormFactor::Laptop);
        assert_eq!(
            form_factor_from_chassis_type(Some("10")),
            FormFactor::Laptop
        );
        assert_eq!(
            form_factor_from_chassis_type(Some("3")),
            FormFactor::Desktop
        );
        assert_eq!(
            form_factor_from_chassis_type(Some("99")),
            FormFactor::Unknown
        );
        assert_eq!(form_factor_from_chassis_type(None), FormFactor::Unknown);
    }

    #[test]
    fn maps_session_types() {
        assert_eq!(session_type_from_env(Some("wayland")), SessionType::Wayland);
        assert_eq!(session_type_from_env(Some("x11")), SessionType::X11);
        assert_eq!(session_type_from_env(Some("tty")), SessionType::Unknown);
        assert_eq!(session_type_from_env(None), SessionType::Unknown);
    }

    #[test]
    fn maps_pci_vendor_ids() {
        assert_eq!(gpu_vendor_from_pci_id(Some("0x10de")), GpuVendor::Nvidia);
        assert_eq!(gpu_vendor_from_pci_id(Some("0x1002")), GpuVendor::Amd);
        assert_eq!(gpu_vendor_from_pci_id(Some("0x8086")), GpuVendor::Intel);
        assert_eq!(gpu_vendor_from_pci_id(Some("0x1234")), GpuVendor::Unknown);
    }

    #[test]
    fn recognizes_drm_card_names() {
        assert!(is_drm_card_name("card0"));
        assert!(is_drm_card_name("card12"));
        assert!(!is_drm_card_name("card0-DP-1"));
        assert!(!is_drm_card_name("renderD128"));
    }
}
