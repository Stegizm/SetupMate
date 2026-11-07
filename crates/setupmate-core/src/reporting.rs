//! Human-readable plan and execution report rendering.
//!
//! User-facing strings are Turkish; identifiers and commands stay English.

use std::fmt::Write;

use crate::executor::ExecutionReport;
use crate::planner::InstallPlan;

/// Renders an install plan as Turkish text.
#[must_use]
pub fn render_plan(plan: &InstallPlan) -> String {
    let mut output = String::new();

    if plan.actions.is_empty() && plan.skipped.is_empty() {
        output.push_str("Öneri yok.\n");
        return output;
    }

    if !plan.actions.is_empty() {
        output.push_str("Kurulum planı (dry-run):\n");
        for action in &plan.actions {
            let _ = writeln!(
                output,
                "- {}: {}",
                action.catalog_id,
                action.command.join(" ")
            );
        }
    }

    if !plan.skipped.is_empty() {
        output.push_str("\nPlandan çıkarılanlar:\n");
        for item in &plan.skipped {
            let _ = writeln!(output, "- {}: {}", item.catalog_id, item.reason.message());
        }
    }

    output
}

/// Renders an execution report as Turkish text.
#[must_use]
pub fn render_report(report: &ExecutionReport) -> String {
    let mut output = String::new();

    if report.entries.is_empty() {
        output.push_str("Çalıştırılacak işlem yok.\n");
        return output;
    }

    if report.applied {
        output.push_str("Uygulanan işlemler:\n");
    } else {
        output.push_str("Dry-run: hiçbir komut çalıştırılmadı.\n");
    }

    for entry in &report.entries {
        let _ = writeln!(
            output,
            "- {}: {}",
            entry.catalog_id,
            entry.command.join(" ")
        );
        if let Some(error) = &entry.error {
            let _ = writeln!(output, "  Hata: {error}");
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::ExecutionEntry;
    use crate::models::{PackageManagerKind, RiskLevel};
    use crate::planner::{PlanAction, SkipReason, SkippedItem};

    fn plan() -> InstallPlan {
        InstallPlan {
            actions: vec![PlanAction {
                catalog_id: "cat-1".to_string(),
                manager: PackageManagerKind::Apt,
                package: "example".to_string(),
                command: vec![
                    "apt-get".to_string(),
                    "install".to_string(),
                    "example".to_string(),
                ],
                risk: RiskLevel::Low,
            }],
            skipped: vec![SkippedItem {
                catalog_id: "cat-2".to_string(),
                reason: SkipReason::TodoVerify,
            }],
        }
    }

    #[test]
    fn renders_actions_and_skipped_items() {
        let text = render_plan(&plan());
        assert!(text.contains("dry-run"));
        assert!(text.contains("cat-1"));
        assert!(text.contains("cat-2"));
        assert!(text.contains("TODO_VERIFY"));
    }

    #[test]
    fn renders_empty_plan() {
        assert_eq!(render_plan(&InstallPlan::default()), "Öneri yok.\n");
    }

    #[test]
    fn renders_dry_run_report_without_execution() {
        let report = ExecutionReport {
            entries: vec![ExecutionEntry {
                catalog_id: "cat-1".to_string(),
                command: vec!["apt-get".to_string(), "install".to_string()],
                output: None,
                error: None,
            }],
            applied: false,
        };

        let text = render_report(&report);
        assert!(text.contains("hiçbir komut çalıştırılmadı"));
        assert!(text.contains("cat-1"));
    }
}
