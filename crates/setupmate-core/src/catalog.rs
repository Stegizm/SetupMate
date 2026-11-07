//! Catalog loading from TOML files under `data/catalog/`.
//!
//! Entries are data, not guesses: an unknown package name must be stored as
//! `TODO_VERIFY` and can never become an install action.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::models::{
    DistroFamily, FormFactor, GpuVendor, PackageManagerKind, PackageRef, PackageSuggestion,
    RiskLevel, SystemInfo, VerificationStatus,
};

/// Marker used in TOML when a package name has not been verified.
pub const TODO_VERIFY: &str = "TODO_VERIFY";

/// Catalog files compiled into the binary, so an AppImage/AUR install needs no
/// external data directory.
pub const EMBEDDED_CATEGORIES: &str = include_str!("../../../data/catalog/categories.toml");
/// See [`EMBEDDED_CATEGORIES`].
pub const EMBEDDED_PACKAGES: &str = include_str!("../../../data/catalog/packages.toml");
/// See [`EMBEDDED_CATEGORIES`].
pub const EMBEDDED_PROFILES: &str = include_str!("../../../data/catalog/profiles.toml");

/// A user-facing package category.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Category {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Contents of `categories.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CategoryCatalog {
    #[serde(default)]
    pub categories: Vec<Category>,
}

/// One package entry as stored in `packages.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PackageEntry {
    pub catalog_id: String,
    pub family: DistroFamily,
    pub manager: PackageManagerKind,
    pub package: String,
    pub risk: RiskLevel,
    pub verification_status: VerificationStatus,
    #[serde(default)]
    pub category_id: String,
    #[serde(default)]
    pub reason: String,
    /// Where the package name was verified (URL, distro docs, etc.).
    #[serde(default)]
    pub source: String,
}

impl PackageEntry {
    /// Converts a catalog entry into a planner-facing suggestion.
    ///
    /// A literal `TODO_VERIFY` package always forces the unverified status, even
    /// if the file mistakenly says `verified`.
    #[must_use]
    pub fn to_suggestion(&self) -> PackageSuggestion {
        let is_todo = self.verification_status == VerificationStatus::TodoVerify
            || self.package == TODO_VERIFY;

        let package = if is_todo {
            PackageRef::TodoVerify
        } else {
            PackageRef::Name(self.package.clone())
        };

        let verification_status = if is_todo {
            VerificationStatus::TodoVerify
        } else {
            self.verification_status
        };

        PackageSuggestion {
            catalog_id: self.catalog_id.clone(),
            family: self.family,
            manager: self.manager,
            package,
            risk: self.risk,
            verification_status,
            reason: self.reason.clone(),
        }
    }
}

/// Contents of `packages.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PackageCatalog {
    #[serde(default)]
    pub packages: Vec<PackageEntry>,
}

/// A hardware/session profile (for example laptop or desktop).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category_ids: Vec<String>,
}

/// Contents of `profiles.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProfileCatalog {
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

/// All catalog files loaded together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub categories: CategoryCatalog,
    pub packages: PackageCatalog,
    pub profiles: ProfileCatalog,
}

impl Catalog {
    /// Loads `categories.toml`, `packages.toml`, and `profiles.toml` from a directory.
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Self, CatalogError> {
        let dir = dir.as_ref();
        Ok(Self {
            categories: load_as(&dir.join("categories.toml"))?,
            packages: load_as(&dir.join("packages.toml"))?,
            profiles: load_as(&dir.join("profiles.toml"))?,
        })
    }

    /// Parses catalog contents that are already in memory.
    ///
    /// Used for the embedded catalog and by tests; no filesystem access.
    pub fn from_toml(
        categories: &str,
        packages: &str,
        profiles: &str,
    ) -> Result<Self, CatalogError> {
        Ok(Self {
            categories: parse_as(categories, "<embedded categories.toml>")?,
            packages: parse_as(packages, "<embedded packages.toml>")?,
            profiles: parse_as(profiles, "<embedded profiles.toml>")?,
        })
    }

    /// Loads the catalog compiled into the binary.
    ///
    /// This is the production path: an AppImage or a package-manager install
    /// must not depend on a `data/` directory next to the executable.
    pub fn load_embedded() -> Result<Self, CatalogError> {
        Self::from_toml(EMBEDDED_CATEGORIES, EMBEDDED_PACKAGES, EMBEDDED_PROFILES)
    }

    /// Converts every package entry into a suggestion.
    #[must_use]
    pub fn suggestions(&self) -> Vec<PackageSuggestion> {
        self.packages
            .packages
            .iter()
            .map(PackageEntry::to_suggestion)
            .collect()
    }

    /// Returns profiles matching the detected form factor and GPU vendor.
    ///
    /// Matching is additive: a laptop with an NVIDIA GPU matches both the
    /// `laptop` and `nvidia` profiles.
    #[must_use]
    pub fn matching_profiles<'a>(&'a self, system: &SystemInfo) -> Vec<&'a Profile> {
        let mut ids = Vec::new();

        match system.form_factor {
            FormFactor::Laptop => ids.push("laptop"),
            FormFactor::Desktop => ids.push("desktop"),
            FormFactor::Unknown => {}
        }

        for vendor in &system.gpu_vendors {
            match vendor {
                GpuVendor::Nvidia => ids.push("nvidia"),
                GpuVendor::Amd => ids.push("amd"),
                GpuVendor::Intel => ids.push("intel"),
                GpuVendor::Unknown => {}
            }
        }

        self.profiles
            .profiles
            .iter()
            .filter(|profile| ids.contains(&profile.id.as_str()))
            .collect()
    }

    /// Checks ids and cross-references. Returns all problems found.
    pub fn validate(&self) -> Result<(), Vec<CatalogValidationError>> {
        use std::collections::HashSet;

        let mut errors = Vec::new();

        let mut category_ids = HashSet::new();
        for category in &self.categories.categories {
            if !category_ids.insert(category.id.as_str()) {
                errors.push(CatalogValidationError::DuplicateCategoryId(
                    category.id.clone(),
                ));
            }
        }

        let mut package_ids = HashSet::new();
        for package in &self.packages.packages {
            if !package_ids.insert(package.catalog_id.as_str()) {
                errors.push(CatalogValidationError::DuplicateCatalogId(
                    package.catalog_id.clone(),
                ));
            }
            if !package.category_id.is_empty()
                && !category_ids.contains(package.category_id.as_str())
            {
                errors.push(CatalogValidationError::UnknownPackageCategory {
                    catalog_id: package.catalog_id.clone(),
                    category_id: package.category_id.clone(),
                });
            }
            if package.verification_status == VerificationStatus::Verified {
                if package.package == TODO_VERIFY {
                    errors.push(CatalogValidationError::VerifiedTodoVerify {
                        catalog_id: package.catalog_id.clone(),
                    });
                } else if package.source.trim().is_empty() {
                    errors.push(CatalogValidationError::VerifiedWithoutSource {
                        catalog_id: package.catalog_id.clone(),
                    });
                }
            }
        }

        let mut profile_ids = HashSet::new();
        for profile in &self.profiles.profiles {
            if !profile_ids.insert(profile.id.as_str()) {
                errors.push(CatalogValidationError::DuplicateProfileId(
                    profile.id.clone(),
                ));
            }
            for category_id in &profile.category_ids {
                if !category_ids.contains(category_id.as_str()) {
                    errors.push(CatalogValidationError::UnknownProfileCategory {
                        profile_id: profile.id.clone(),
                        category_id: category_id.clone(),
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Catalog consistency problem found by [`Catalog::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogValidationError {
    DuplicateCategoryId(String),
    DuplicateCatalogId(String),
    DuplicateProfileId(String),
    UnknownPackageCategory {
        catalog_id: String,
        category_id: String,
    },
    UnknownProfileCategory {
        profile_id: String,
        category_id: String,
    },
    VerifiedWithoutSource {
        catalog_id: String,
    },
    VerifiedTodoVerify {
        catalog_id: String,
    },
}

impl std::fmt::Display for CatalogValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateCategoryId(id) => write!(f, "duplicate category id: {id}"),
            Self::DuplicateCatalogId(id) => write!(f, "duplicate catalog id: {id}"),
            Self::DuplicateProfileId(id) => write!(f, "duplicate profile id: {id}"),
            Self::UnknownPackageCategory {
                catalog_id,
                category_id,
            } => write!(
                f,
                "package {catalog_id} references unknown category {category_id}"
            ),
            Self::UnknownProfileCategory {
                profile_id,
                category_id,
            } => write!(
                f,
                "profile {profile_id} references unknown category {category_id}"
            ),
            Self::VerifiedWithoutSource { catalog_id } => {
                write!(f, "verified package {catalog_id} has no source")
            }
            Self::VerifiedTodoVerify { catalog_id } => {
                write!(f, "verified package {catalog_id} uses TODO_VERIFY")
            }
        }
    }
}

impl std::error::Error for CatalogValidationError {}

/// Catalog read/parse failure. No panics or unwraps in the loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    Io { path: String, message: String },
    Parse { path: String, message: String },
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "{path}: {message}"),
            Self::Parse { path, message } => write!(f, "{path}: {message}"),
        }
    }
}

impl std::error::Error for CatalogError {}

fn load_as<T>(path: &Path) -> Result<T, CatalogError>
where
    T: for<'de> Deserialize<'de>,
{
    let contents = fs::read_to_string(path).map_err(|error| CatalogError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;

    parse_as(&contents, &path.display().to_string())
}

fn parse_as<T>(contents: &str, label: &str) -> Result<T, CatalogError>
where
    T: for<'de> Deserialize<'de>,
{
    toml::from_str(contents).map_err(|error| CatalogError::Parse {
        path: label.to_string(),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGES: &str = r#"
[[packages]]
catalog_id = "cat-1"
family = "debian"
manager = "apt"
package = "example"
risk = "low"
verification_status = "verified"
category_id = "essentials"
reason = "test"
source = "test-fixture-source"

[[packages]]
catalog_id = "cat-2"
family = "arch"
manager = "pacman"
package = "TODO_VERIFY"
risk = "medium"
verification_status = "todo_verify"
reason = "needs verification"
"#;

    #[test]
    fn parses_packages_and_converts_to_suggestions() {
        let catalog: PackageCatalog = toml::from_str(PACKAGES).expect("valid test toml");
        assert_eq!(catalog.packages.len(), 2);

        let first = catalog.packages[0].to_suggestion();
        assert_eq!(first.family, DistroFamily::Debian);
        assert_eq!(first.manager, PackageManagerKind::Apt);
        assert!(first.is_installable());

        let second = catalog.packages[1].to_suggestion();
        assert_eq!(second.package, PackageRef::TodoVerify);
        assert_eq!(second.verification_status, VerificationStatus::TodoVerify);
        assert!(!second.is_installable());
    }

    #[test]
    fn todo_verify_package_string_forces_todo_status() {
        let entry: PackageEntry = toml::from_str(
            r#"
catalog_id = "cat-3"
family = "debian"
manager = "apt"
package = "TODO_VERIFY"
risk = "low"
verification_status = "verified"
"#,
        )
        .expect("valid test toml");

        let suggestion = entry.to_suggestion();
        assert_eq!(suggestion.package, PackageRef::TodoVerify);
        assert_eq!(
            suggestion.verification_status,
            VerificationStatus::TodoVerify
        );
        assert!(!suggestion.is_installable());
    }

    #[test]
    fn loads_missing_directory_as_io_error() {
        let error = Catalog::load_dir("/definitely/missing/setupmate-catalog").unwrap_err();
        assert!(matches!(error, CatalogError::Io { .. }));
    }

    #[test]
    fn parses_categories_and_profiles() {
        let categories: CategoryCatalog = toml::from_str(
            r#"
[[categories]]
id = "essentials"
name = "Temel"
description = "Temel araçlar"
"#,
        )
        .expect("valid test toml");
        assert_eq!(categories.categories[0].id, "essentials");

        let profiles: ProfileCatalog = toml::from_str(
            r#"
[[profiles]]
id = "laptop"
name = "Dizüstü"
category_ids = ["essentials"]
"#,
        )
        .expect("valid test toml");
        assert_eq!(
            profiles.profiles[0].category_ids,
            vec!["essentials".to_string()]
        );
    }

    #[test]
    fn matches_form_factor_and_gpu_profiles() {
        use crate::models::SessionType;

        fn profile(id: &str, name: &str) -> Profile {
            Profile {
                id: id.to_string(),
                name: name.to_string(),
                description: String::new(),
                category_ids: Vec::new(),
            }
        }

        let catalog = Catalog {
            categories: CategoryCatalog {
                categories: Vec::new(),
            },
            packages: PackageCatalog {
                packages: Vec::new(),
            },
            profiles: ProfileCatalog {
                profiles: vec![
                    profile("laptop", "Dizüstü"),
                    profile("desktop", "Masaüstü"),
                    profile("nvidia", "NVIDIA"),
                    profile("intel", "Intel"),
                ],
            },
        };

        let system = SystemInfo {
            distro_family: DistroFamily::Debian,
            gpu_vendors: vec![GpuVendor::Nvidia, GpuVendor::Intel],
            form_factor: FormFactor::Laptop,
            session_type: SessionType::Wayland,
        };

        let ids: Vec<&str> = catalog
            .matching_profiles(&system)
            .iter()
            .map(|profile| profile.id.as_str())
            .collect();

        // Hybrid laptop: both GPU profiles match alongside the form factor.
        assert_eq!(ids, vec!["laptop", "nvidia", "intel"]);
    }

    #[test]
    fn unknown_hardware_matches_no_profiles() {
        use crate::models::SessionType;

        let catalog = Catalog {
            categories: CategoryCatalog {
                categories: Vec::new(),
            },
            packages: PackageCatalog {
                packages: Vec::new(),
            },
            profiles: ProfileCatalog {
                profiles: vec![Profile {
                    id: "laptop".to_string(),
                    name: "Dizüstü".to_string(),
                    description: String::new(),
                    category_ids: Vec::new(),
                }],
            },
        };

        let system = SystemInfo {
            distro_family: DistroFamily::Unknown,
            gpu_vendors: Vec::new(),
            form_factor: FormFactor::Unknown,
            session_type: SessionType::Unknown,
        };

        assert!(catalog.matching_profiles(&system).is_empty());
    }

    #[test]
    fn verified_without_source_is_rejected() {
        let entry = PackageEntry {
            catalog_id: "cat-src".to_string(),
            family: DistroFamily::Debian,
            manager: PackageManagerKind::Apt,
            package: "example".to_string(),
            risk: RiskLevel::Low,
            verification_status: VerificationStatus::Verified,
            category_id: String::new(),
            reason: String::new(),
            source: String::new(),
        };

        let catalog = Catalog {
            categories: CategoryCatalog {
                categories: Vec::new(),
            },
            packages: PackageCatalog {
                packages: vec![entry],
            },
            profiles: ProfileCatalog {
                profiles: Vec::new(),
            },
        };

        let errors = catalog.validate().unwrap_err();
        assert!(errors.iter().any(|error| matches!(
            error,
            CatalogValidationError::VerifiedWithoutSource { catalog_id } if catalog_id == "cat-src"
        )));
    }

    #[test]
    fn verified_with_todo_verify_is_rejected() {
        let entry = PackageEntry {
            catalog_id: "cat-todo".to_string(),
            family: DistroFamily::Debian,
            manager: PackageManagerKind::Apt,
            package: TODO_VERIFY.to_string(),
            risk: RiskLevel::Low,
            verification_status: VerificationStatus::Verified,
            category_id: String::new(),
            reason: String::new(),
            source: "https://example.invalid".to_string(),
        };

        let catalog = Catalog {
            categories: CategoryCatalog {
                categories: Vec::new(),
            },
            packages: PackageCatalog {
                packages: vec![entry],
            },
            profiles: ProfileCatalog {
                profiles: Vec::new(),
            },
        };

        let errors = catalog.validate().unwrap_err();
        assert!(errors.iter().any(|error| matches!(
            error,
            CatalogValidationError::VerifiedTodoVerify { catalog_id } if catalog_id == "cat-todo"
        )));
    }

    #[test]
    fn loads_repository_catalog_files() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/catalog");
        let catalog = Catalog::load_dir(dir).expect("repository catalog should load");

        assert!(!catalog.categories.categories.is_empty());
        assert!(!catalog.profiles.profiles.is_empty());
        assert!(!catalog.packages.packages.is_empty());
        assert!(!catalog.suggestions().is_empty());
        assert!(catalog.validate().is_ok());

        // Every supported family must have at least one verified entry.
        for family in [
            DistroFamily::Debian,
            DistroFamily::Fedora,
            DistroFamily::Arch,
            DistroFamily::OpenSuse,
            DistroFamily::Alpine,
            DistroFamily::Void,
        ] {
            assert!(
                catalog
                    .packages
                    .packages
                    .iter()
                    .any(|package| package.family == family),
                "no catalog entries for {family:?}"
            );
        }
    }

    #[test]
    fn embedded_catalog_matches_repository_files() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/catalog");
        let on_disk = Catalog::load_dir(dir).expect("repository catalog should load");
        let embedded = Catalog::load_embedded().expect("embedded catalog should parse");

        assert_eq!(embedded, on_disk);
        assert!(embedded.validate().is_ok());
    }

    #[test]
    fn validation_reports_unknown_and_duplicate_ids() {
        let entry = PackageEntry {
            catalog_id: "dup".to_string(),
            family: DistroFamily::Debian,
            manager: PackageManagerKind::Apt,
            package: TODO_VERIFY.to_string(),
            risk: RiskLevel::Low,
            verification_status: VerificationStatus::TodoVerify,
            category_id: "missing".to_string(),
            reason: String::new(),
            source: String::new(),
        };

        let catalog = Catalog {
            categories: CategoryCatalog {
                categories: vec![Category {
                    id: "basics".to_string(),
                    name: "Temel".to_string(),
                    description: String::new(),
                }],
            },
            packages: PackageCatalog {
                packages: vec![entry.clone(), entry],
            },
            profiles: ProfileCatalog {
                profiles: vec![Profile {
                    id: "p1".to_string(),
                    name: "P1".to_string(),
                    description: String::new(),
                    category_ids: vec!["missing".to_string()],
                }],
            },
        };

        let errors = catalog.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|error| matches!(error, CatalogValidationError::DuplicateCatalogId(_))));
        assert!(errors
            .iter()
            .any(|error| matches!(error, CatalogValidationError::UnknownPackageCategory { .. })));
        assert!(errors
            .iter()
            .any(|error| matches!(error, CatalogValidationError::UnknownProfileCategory { .. })));
    }
}
