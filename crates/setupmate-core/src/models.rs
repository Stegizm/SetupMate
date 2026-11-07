//! Core data models shared by detection, catalog, planner, and GUI.
//!
//! These are types only; no detection or catalog logic lives here yet.

use serde::Deserialize;

/// Linux distribution family, used to choose a package manager and catalog entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DistroFamily {
    Debian,
    Fedora,
    Arch,
    OpenSuse,
    Alpine,
    Void,
    Unknown,
}

/// GPU vendor. Driver packages are high-risk and guide-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

/// Machine form factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormFactor {
    Laptop,
    Desktop,
    Unknown,
}

/// Graphical session protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    Wayland,
    X11,
    Unknown,
}

/// Read-only facts detected from the running system.
///
/// `gpu_vendors` holds every distinct vendor found in `/sys/class/drm`, in
/// directory order. A hybrid laptop (e.g. Intel iGPU + NVIDIA dGPU) lists both;
/// an empty list means no known vendor was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemInfo {
    pub distro_family: DistroFamily,
    pub gpu_vendors: Vec<GpuVendor>,
    pub form_factor: FormFactor,
    pub session_type: SessionType,
}

/// Supported package manager kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManagerKind {
    Apt,
    Dnf,
    Pacman,
    Zypper,
    Apk,
    Xbps,
    Unknown,
}

/// Risk classification for a suggested package or action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Whether a package name has been verified against a catalog source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    TodoVerify,
}

/// A package reference is either a verified name or explicitly unverified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageRef {
    Name(String),
    TodoVerify,
}

/// A catalog-backed package suggestion using the project's required fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSuggestion {
    pub catalog_id: String,
    pub family: DistroFamily,
    pub manager: PackageManagerKind,
    pub package: PackageRef,
    pub risk: RiskLevel,
    pub verification_status: VerificationStatus,
    pub reason: String,
}

impl PackageSuggestion {
    /// Returns true only for verified, named, non-high-risk packages.
    ///
    /// This encodes the safety rules: `TODO_VERIFY` entries are excluded from
    /// install plans and high-risk packages are guide-only.
    #[must_use]
    pub fn is_installable(&self) -> bool {
        self.verification_status == VerificationStatus::Verified
            && self.risk != RiskLevel::High
            && matches!(self.package, PackageRef::Name(_))
    }
}
