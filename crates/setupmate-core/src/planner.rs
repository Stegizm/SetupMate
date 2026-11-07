//! Install plan generation.
//!
//! Plans are descriptions only; building a plan never executes anything.
//! `TODO_VERIFY` and high-risk entries are always excluded.

use crate::models::{
    PackageManagerKind, PackageRef, PackageSuggestion, RiskLevel, SystemInfo, VerificationStatus,
};
use crate::package_managers;

/// A single install step with its exact command vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanAction {
    pub catalog_id: String,
    /// Manager used to rebuild a non-interactive command for the live path.
    pub manager: PackageManagerKind,
    /// Verified package name.
    pub package: String,
    pub command: Vec<String>,
    pub risk: RiskLevel,
}

/// Why a suggestion was excluded from the install plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    TodoVerify,
    HighRisk,
    FamilyMismatch,
    UnknownPackageManager,
}

impl SkipReason {
    /// Turkish explanation shown to the user.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::TodoVerify => "Paket adı doğrulanmadı (TODO_VERIFY); kurulum planına alınmadı.",
            Self::HighRisk => "Yüksek riskli işlem; yalnızca rehber olarak gösterilir.",
            Self::FamilyMismatch => "Paket bu dağıtım ailesi için değil; plana alınmadı.",
            Self::UnknownPackageManager => "Paket yöneticisi bilinmiyor; komut üretilmedi.",
        }
    }
}

/// A suggestion that was left out of the plan, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedItem {
    pub catalog_id: String,
    pub reason: SkipReason,
}

/// A dry-run install plan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstallPlan {
    pub actions: Vec<PlanAction>,
    pub skipped: Vec<SkippedItem>,
}

/// Builds a dry-run install plan for the given suggestions.
///
/// This never executes commands and always excludes `TODO_VERIFY` and
/// high-risk entries.
#[must_use]
pub fn build_plan(system: &SystemInfo, suggestions: &[PackageSuggestion]) -> InstallPlan {
    let mut plan = InstallPlan::default();

    for suggestion in suggestions {
        match evaluate(system, suggestion) {
            Ok(action) => plan.actions.push(action),
            Err(reason) => plan.skipped.push(SkippedItem {
                catalog_id: suggestion.catalog_id.clone(),
                reason,
            }),
        }
    }

    plan
}

/// Rewrites an approved plan into the commands that are actually executed.
///
/// The displayed plan stays plain; the live commands add the manager's
/// non-interactive flag. Building this never executes anything, and entries that
/// cannot be mapped are dropped rather than run interactively.
#[must_use]
pub fn live_plan(plan: &InstallPlan) -> InstallPlan {
    let actions = plan
        .actions
        .iter()
        .filter(|action| {
            // Defence in depth: the preview plan already excludes these, but the
            // live path must never run an unverified or high-risk entry either.
            action.risk != RiskLevel::High
                && !action.package.is_empty()
                && action.package != crate::catalog::TODO_VERIFY
        })
        .filter_map(|action| {
            let command =
                package_managers::non_interactive_command(action.manager, &action.package)?;
            Some(PlanAction {
                command,
                ..action.clone()
            })
        })
        .collect();

    InstallPlan {
        actions,
        skipped: plan.skipped.clone(),
    }
}

fn evaluate(system: &SystemInfo, suggestion: &PackageSuggestion) -> Result<PlanAction, SkipReason> {
    if suggestion.verification_status == VerificationStatus::TodoVerify {
        return Err(SkipReason::TodoVerify);
    }

    if suggestion.risk == RiskLevel::High {
        return Err(SkipReason::HighRisk);
    }

    if suggestion.family != system.distro_family {
        return Err(SkipReason::FamilyMismatch);
    }

    let package = match &suggestion.package {
        PackageRef::Name(name) => name,
        PackageRef::TodoVerify => return Err(SkipReason::TodoVerify),
    };

    if suggestion.manager == PackageManagerKind::Unknown {
        return Err(SkipReason::UnknownPackageManager);
    }

    let command = package_managers::install_command(suggestion.manager, package)
        .ok_or(SkipReason::UnknownPackageManager)?;

    Ok(PlanAction {
        catalog_id: suggestion.catalog_id.clone(),
        manager: suggestion.manager,
        package: package.clone(),
        command,
        risk: suggestion.risk,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        DistroFamily, FormFactor, GpuVendor, PackageManagerKind, PackageRef, PackageSuggestion,
        RiskLevel, SessionType, SystemInfo, VerificationStatus,
    };

    fn system() -> SystemInfo {
        SystemInfo {
            distro_family: DistroFamily::Debian,
            gpu_vendors: vec![GpuVendor::Intel],
            form_factor: FormFactor::Laptop,
            session_type: SessionType::Wayland,
        }
    }

    fn suggestion(
        catalog_id: &str,
        family: DistroFamily,
        package: PackageRef,
        risk: RiskLevel,
        status: VerificationStatus,
    ) -> PackageSuggestion {
        PackageSuggestion {
            catalog_id: catalog_id.to_string(),
            family,
            manager: PackageManagerKind::Apt,
            package,
            risk,
            verification_status: status,
            reason: "test".to_string(),
        }
    }

    #[test]
    fn verified_low_risk_package_becomes_action() {
        let plan = build_plan(
            &system(),
            &[suggestion(
                "cat-1",
                DistroFamily::Debian,
                PackageRef::Name("example".to_string()),
                RiskLevel::Low,
                VerificationStatus::Verified,
            )],
        );

        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.skipped.len(), 0);
        assert_eq!(plan.actions[0].command[0], "apt-get");
    }

    #[test]
    fn todo_verify_is_excluded() {
        let plan = build_plan(
            &system(),
            &[suggestion(
                "cat-1",
                DistroFamily::Debian,
                PackageRef::TodoVerify,
                RiskLevel::Low,
                VerificationStatus::TodoVerify,
            )],
        );

        assert!(plan.actions.is_empty());
        assert_eq!(plan.skipped[0].reason, SkipReason::TodoVerify);
    }

    #[test]
    fn high_risk_is_excluded_even_when_verified() {
        let plan = build_plan(
            &system(),
            &[suggestion(
                "gpu-1",
                DistroFamily::Debian,
                PackageRef::Name("example".to_string()),
                RiskLevel::High,
                VerificationStatus::Verified,
            )],
        );

        assert!(plan.actions.is_empty());
        assert_eq!(plan.skipped[0].reason, SkipReason::HighRisk);
    }

    #[test]
    fn family_mismatch_is_excluded() {
        let plan = build_plan(
            &system(),
            &[suggestion(
                "cat-1",
                DistroFamily::Arch,
                PackageRef::Name("example".to_string()),
                RiskLevel::Low,
                VerificationStatus::Verified,
            )],
        );

        assert!(plan.actions.is_empty());
        assert_eq!(plan.skipped[0].reason, SkipReason::FamilyMismatch);
    }

    #[test]
    fn unknown_manager_is_excluded() {
        let mut item = suggestion(
            "cat-1",
            DistroFamily::Debian,
            PackageRef::Name("example".to_string()),
            RiskLevel::Low,
            VerificationStatus::Verified,
        );
        item.manager = PackageManagerKind::Unknown;

        let plan = build_plan(&system(), &[item]);
        assert!(plan.actions.is_empty());
        assert_eq!(plan.skipped[0].reason, SkipReason::UnknownPackageManager);
    }

    #[test]
    fn live_plan_adds_the_non_interactive_flag() {
        let plan = build_plan(
            &system(),
            &[suggestion(
                "cat-1",
                DistroFamily::Debian,
                PackageRef::Name("example".to_string()),
                RiskLevel::Low,
                VerificationStatus::Verified,
            )],
        );

        let live = live_plan(&plan);
        assert_eq!(
            live.actions[0].command,
            vec![
                "apt-get".to_string(),
                "install".to_string(),
                "-y".to_string(),
                "--no-install-recommends".to_string(),
                "example".to_string(),
            ]
        );
        // The displayed plan is untouched.
        assert!(!plan.actions[0].command.iter().any(|part| part == "-y"));
    }

    #[test]
    fn live_plan_drops_high_risk_and_todo_entries() {
        let plan = InstallPlan {
            actions: vec![
                PlanAction {
                    catalog_id: "high".to_string(),
                    manager: PackageManagerKind::Apt,
                    package: "example".to_string(),
                    command: vec!["apt-get".to_string(), "install".to_string()],
                    risk: RiskLevel::High,
                },
                PlanAction {
                    catalog_id: "todo".to_string(),
                    manager: PackageManagerKind::Apt,
                    package: crate::catalog::TODO_VERIFY.to_string(),
                    command: vec!["apt-get".to_string(), "install".to_string()],
                    risk: RiskLevel::Low,
                },
            ],
            skipped: Vec::new(),
        };

        assert!(live_plan(&plan).actions.is_empty());
    }
}
