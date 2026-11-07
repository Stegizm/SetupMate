//! End-to-end dry-run flow using the repository catalog.

use std::path::PathBuf;

use setupmate_core::catalog::Catalog;
use setupmate_core::detection;
use setupmate_core::executor::{self, DryRunRunner, ExecutionMode};
use setupmate_core::models::{
    DistroFamily, FormFactor, GpuVendor, PackageManagerKind, PackageRef, PackageSuggestion,
    RiskLevel, SessionType, SystemInfo, VerificationStatus,
};
use setupmate_core::planner;
use setupmate_core::planner::SkipReason;
use setupmate_core::reporting;

fn catalog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/catalog")
}

#[test]
fn end_to_end_dry_run_flow() {
    let catalog = Catalog::load_dir(catalog_dir()).expect("catalog loads");
    assert!(catalog.validate().is_ok());

    // Detection must not panic and always returns a complete SystemInfo.
    let _detected = detection::detect();

    let system = SystemInfo {
        distro_family: DistroFamily::Debian,
        gpu_vendors: vec![GpuVendor::Intel],
        form_factor: FormFactor::Laptop,
        session_type: SessionType::Wayland,
    };

    let suggestions = vec![PackageSuggestion {
        catalog_id: "todo.example".to_string(),
        family: DistroFamily::Debian,
        manager: PackageManagerKind::Apt,
        package: PackageRef::TodoVerify,
        risk: RiskLevel::Low,
        verification_status: VerificationStatus::TodoVerify,
        reason: "unverified test fixture".to_string(),
    }];

    let plan = planner::build_plan(&system, &suggestions);
    assert!(plan.actions.is_empty());
    assert_eq!(plan.skipped.len(), 1);

    let mut runner = DryRunRunner::default();
    let report = executor::execute_plan(&plan, &mut runner, ExecutionMode::DryRun, None);
    assert!(!report.applied);
    assert!(runner.commands().is_empty());

    let text = reporting::render_plan(&plan);
    assert!(text.contains("TODO_VERIFY"));

    // The shipped catalog now contains verified entries.
    assert!(!catalog.suggestions().is_empty());
    assert!(catalog
        .suggestions()
        .iter()
        .any(|suggestion| suggestion.verification_status == VerificationStatus::Verified));
}

#[test]
fn repository_catalog_produces_arch_plan() {
    let catalog = Catalog::load_dir(catalog_dir()).expect("catalog loads");
    assert!(catalog.validate().is_ok());

    let system = SystemInfo {
        distro_family: DistroFamily::Arch,
        gpu_vendors: vec![GpuVendor::Intel],
        form_factor: FormFactor::Laptop,
        session_type: SessionType::Wayland,
    };

    let suggestions = family_suggestions(&catalog, DistroFamily::Arch);
    let plan = planner::build_plan(&system, &suggestions);

    assert!(!plan.actions.is_empty());
    assert!(plan
        .actions
        .iter()
        .all(|action| action.command.first().map(String::as_str) == Some("pacman")));
    // The GPU guide entry is intentionally TODO_VERIFY.
    assert!(!plan.skipped.is_empty());
    assert!(plan
        .skipped
        .iter()
        .all(|item| item.reason == SkipReason::TodoVerify));
}

#[test]
fn repository_catalog_produces_plan_for_every_family() {
    let catalog = Catalog::load_dir(catalog_dir()).expect("catalog loads");
    assert!(catalog.validate().is_ok());

    let cases = [
        (DistroFamily::Debian, "apt-get"),
        (DistroFamily::Fedora, "dnf"),
        (DistroFamily::Arch, "pacman"),
        (DistroFamily::OpenSuse, "zypper"),
        (DistroFamily::Alpine, "apk"),
        (DistroFamily::Void, "xbps-install"),
    ];

    for (family, executable) in cases {
        let system = SystemInfo {
            distro_family: family,
            gpu_vendors: vec![GpuVendor::Intel],
            form_factor: FormFactor::Laptop,
            session_type: SessionType::Wayland,
        };
        let suggestions = family_suggestions(&catalog, family);
        assert!(!suggestions.is_empty(), "no entries for {family:?}");

        let plan = planner::build_plan(&system, &suggestions);
        assert!(!plan.actions.is_empty(), "no actions for {family:?}");
        assert!(
            !plan.skipped.is_empty(),
            "missing GPU guide skip for {family:?}"
        );
        assert!(
            plan.skipped
                .iter()
                .all(|item| item.reason == SkipReason::TodoVerify),
            "unexpected skip reasons for {family:?}: {:?}",
            plan.skipped
        );
        assert!(
            plan.actions
                .iter()
                .all(|action| action.command.first().map(String::as_str) == Some(executable)),
            "wrong executable for {family:?}"
        );
    }
}

fn family_suggestions(catalog: &Catalog, family: DistroFamily) -> Vec<PackageSuggestion> {
    catalog
        .suggestions()
        .into_iter()
        .filter(|suggestion| suggestion.family == family)
        .collect()
}
