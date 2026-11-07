//! Command execution behind a `CommandRunner` trait.
//!
//! The default remains dry-run. Real execution requires both
//! `ExecutionMode::Apply` and an explicit `Approval` token, so a plan can never
//! run by accident. The GUI collects the approval in a separate confirmation
//! dialog.

use std::process::{Command, Stdio};

use crate::planner::InstallPlan;
use crate::sys;

/// Result of running one external command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    /// Whether the command exited successfully.
    #[must_use]
    pub fn success(&self) -> bool {
        self.status_code == Some(0)
    }

    /// Short Turkish explanation used when a command fails.
    #[must_use]
    pub fn failure_message(&self) -> String {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return stderr.to_string();
        }
        match self.status_code {
            Some(code) => format!("Komut başarısız oldu (çıkış kodu {code})."),
            None => "Komut bir sinyalle sonlandı.".to_string(),
        }
    }
}

/// Abstraction over external command execution; tests use a mock.
pub trait CommandRunner {
    fn run(&mut self, command: &[String]) -> Result<CommandOutput, String>;
}

/// Runner that records commands without executing anything.
#[derive(Debug, Default)]
pub struct DryRunRunner {
    commands: Vec<Vec<String>>,
}

impl DryRunRunner {
    /// Commands observed so far, in call order.
    #[must_use]
    pub fn commands(&self) -> &[Vec<String>] {
        &self.commands
    }
}

impl CommandRunner for DryRunRunner {
    fn run(&mut self, command: &[String]) -> Result<CommandOutput, String> {
        self.commands.push(command.to_vec());
        Ok(CommandOutput {
            status_code: Some(0),
            stdout: format!("[dry-run] {}", command.join(" ")),
            stderr: String::new(),
        })
    }
}

/// Runner that executes commands on the real system.
///
/// When the app is not running as root, commands are wrapped in `pkexec` so the
/// user gets the desktop polkit prompt. Missing `pkexec` is reported as an error
/// instead of silently trying to run a privileged command.
#[derive(Debug, Clone, Default)]
pub struct SystemRunner {
    prefix: Vec<String>,
}

impl SystemRunner {
    /// Runs commands directly (used when already root or in tests).
    #[must_use]
    pub fn direct() -> Self {
        Self::default()
    }

    /// Runs commands through the given prefix (for example `["pkexec"]`).
    #[must_use]
    pub fn with_prefix(prefix: Vec<String>) -> Self {
        Self { prefix }
    }

    /// Runs commands through `pkexec` when needed, or directly as root.
    pub fn elevated() -> Result<Self, String> {
        Ok(Self::with_prefix(sys::elevation_prefix()?))
    }
}

impl CommandRunner for SystemRunner {
    fn run(&mut self, command: &[String]) -> Result<CommandOutput, String> {
        let (program, args) = command
            .split_first()
            .ok_or_else(|| "Boş komut çalıştırılamaz.".to_string())?;

        // `pkexec` sanitises the environment, so pass the noninteractive marker
        // through `env` instead of relying on `.env()`.
        let mut process = match self.prefix.split_first() {
            Some((helper, helper_args)) => {
                let mut process = Command::new(helper);
                process
                    .args(helper_args)
                    .arg("env")
                    .arg("DEBIAN_FRONTEND=noninteractive")
                    .arg(program);
                process
            }
            None => {
                let mut process = Command::new("env");
                process.arg("DEBIAN_FRONTEND=noninteractive").arg(program);
                process
            }
        };

        process
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = process
            .output()
            .map_err(|error| format!("`{program}` çalıştırılamadı: {error}"))?;

        Ok(CommandOutput {
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// How a plan should be executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    /// Only describe commands; never execute. This is the default.
    #[default]
    DryRun,
    /// Execute commands, but only together with an `Approval`.
    Apply,
}

/// Proof that the user explicitly approved execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approval;

impl Approval {
    /// Must only be constructed after an explicit user confirmation.
    #[must_use]
    pub fn explicit() -> Self {
        Self
    }
}

/// Result of one planned action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionEntry {
    pub catalog_id: String,
    pub command: Vec<String>,
    pub output: Option<CommandOutput>,
    pub error: Option<String>,
}

/// Result of executing (or simulating) a whole plan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionReport {
    pub entries: Vec<ExecutionEntry>,
    pub applied: bool,
}

impl ExecutionReport {
    /// Whether every applied entry finished without an error.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        !self.applied || self.entries.iter().all(|entry| entry.error.is_none())
    }
}

/// Executes or simulates an install plan.
///
/// Commands are only passed to the runner when `mode` is `Apply` *and* an
/// `Approval` is present. Otherwise this returns a report with no execution.
#[must_use]
pub fn execute_plan<R: CommandRunner>(
    plan: &InstallPlan,
    runner: &mut R,
    mode: ExecutionMode,
    approval: Option<Approval>,
) -> ExecutionReport {
    let should_apply = mode == ExecutionMode::Apply && approval.is_some();
    let mut report = ExecutionReport {
        entries: Vec::with_capacity(plan.actions.len()),
        applied: should_apply,
    };

    for action in &plan.actions {
        let (output, error) = run_action(action.command.as_slice(), runner, should_apply);
        report.entries.push(ExecutionEntry {
            catalog_id: action.catalog_id.clone(),
            command: action.command.clone(),
            output,
            error,
        });
    }

    report
}

/// Executes a plan and reports every entry as it finishes.
///
/// Unlike [`execute_plan`], this stops at the first failure so a broken package
/// manager does not cascade into more commands. The callback runs after each
/// entry, which the GUI uses to update its progress log.
pub fn execute_plan_with_progress<R, F>(
    plan: &InstallPlan,
    runner: &mut R,
    mode: ExecutionMode,
    approval: Option<Approval>,
    mut on_entry: F,
) -> ExecutionReport
where
    R: CommandRunner,
    F: FnMut(&ExecutionEntry),
{
    let should_apply = mode == ExecutionMode::Apply && approval.is_some();
    let mut report = ExecutionReport {
        entries: Vec::with_capacity(plan.actions.len()),
        applied: should_apply,
    };

    for action in &plan.actions {
        let (output, error) = run_action(action.command.as_slice(), runner, should_apply);
        let failed = should_apply && error.is_some();

        report.entries.push(ExecutionEntry {
            catalog_id: action.catalog_id.clone(),
            command: action.command.clone(),
            output,
            error,
        });

        if let Some(entry) = report.entries.last() {
            on_entry(entry);
        }

        if failed {
            break;
        }
    }

    report
}

fn run_action<R: CommandRunner>(
    command: &[String],
    runner: &mut R,
    should_apply: bool,
) -> (Option<CommandOutput>, Option<String>) {
    if !should_apply {
        return (None, None);
    }

    match runner.run(command) {
        Ok(output) if output.success() => (Some(output), None),
        Ok(output) => {
            let message = output.failure_message();
            (Some(output), Some(message))
        }
        Err(error) => (None, Some(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PackageManagerKind, RiskLevel};
    use crate::planner::PlanAction;

    #[derive(Default)]
    struct MockRunner {
        calls: Vec<Vec<String>>,
        fail: bool,
        exit_code: i32,
    }

    impl CommandRunner for MockRunner {
        fn run(&mut self, command: &[String]) -> Result<CommandOutput, String> {
            self.calls.push(command.to_vec());
            if self.fail {
                Err("mock failure".to_string())
            } else {
                Ok(CommandOutput {
                    status_code: Some(self.exit_code),
                    stdout: String::new(),
                    stderr: if self.exit_code == 0 {
                        String::new()
                    } else {
                        "mock stderr".to_string()
                    },
                })
            }
        }
    }

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
            skipped: Vec::new(),
        }
    }

    fn two_action_plan() -> InstallPlan {
        let mut plan = plan();
        plan.actions.push(PlanAction {
            catalog_id: "cat-2".to_string(),
            manager: PackageManagerKind::Apt,
            package: "second".to_string(),
            command: vec![
                "apt-get".to_string(),
                "install".to_string(),
                "second".to_string(),
            ],
            risk: RiskLevel::Low,
        });
        plan
    }

    #[test]
    fn dry_run_never_calls_runner() {
        let mut runner = MockRunner::default();
        let report = execute_plan(
            &plan(),
            &mut runner,
            ExecutionMode::DryRun,
            Some(Approval::explicit()),
        );

        assert!(!report.applied);
        assert!(runner.calls.is_empty());
        assert_eq!(report.entries.len(), 1);
    }

    #[test]
    fn apply_without_approval_never_calls_runner() {
        let mut runner = MockRunner::default();
        let report = execute_plan(&plan(), &mut runner, ExecutionMode::Apply, None);

        assert!(!report.applied);
        assert!(runner.calls.is_empty());
    }

    #[test]
    fn apply_with_approval_calls_runner() {
        let mut runner = MockRunner::default();
        let report = execute_plan(
            &plan(),
            &mut runner,
            ExecutionMode::Apply,
            Some(Approval::explicit()),
        );

        assert!(report.applied);
        assert_eq!(runner.calls.len(), 1);
        assert!(report.entries[0].error.is_none());
        assert!(report.succeeded());
    }

    #[test]
    fn runner_error_is_captured() {
        let mut runner = MockRunner {
            fail: true,
            ..MockRunner::default()
        };
        let report = execute_plan(
            &plan(),
            &mut runner,
            ExecutionMode::Apply,
            Some(Approval::explicit()),
        );

        assert_eq!(report.entries[0].error.as_deref(), Some("mock failure"));
        assert!(!report.succeeded());
    }

    #[test]
    fn nonzero_exit_is_an_error() {
        let mut runner = MockRunner {
            exit_code: 100,
            ..MockRunner::default()
        };
        let report = execute_plan(
            &plan(),
            &mut runner,
            ExecutionMode::Apply,
            Some(Approval::explicit()),
        );

        assert_eq!(report.entries[0].error.as_deref(), Some("mock stderr"));
        assert!(!report.succeeded());
    }

    #[test]
    fn progress_stops_at_first_failure() {
        let mut runner = MockRunner {
            exit_code: 1,
            ..MockRunner::default()
        };
        let mut seen = Vec::new();

        let report = execute_plan_with_progress(
            &two_action_plan(),
            &mut runner,
            ExecutionMode::Apply,
            Some(Approval::explicit()),
            |entry| seen.push(entry.catalog_id.clone()),
        );

        assert_eq!(runner.calls.len(), 1);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(seen, vec!["cat-1".to_string()]);
    }

    #[test]
    fn progress_visits_every_entry_on_success() {
        let mut runner = MockRunner::default();
        let mut seen = Vec::new();

        let report = execute_plan_with_progress(
            &two_action_plan(),
            &mut runner,
            ExecutionMode::Apply,
            Some(Approval::explicit()),
            |entry| seen.push(entry.catalog_id.clone()),
        );

        assert_eq!(seen, vec!["cat-1".to_string(), "cat-2".to_string()]);
        assert!(report.succeeded());
    }

    #[test]
    fn dry_run_runner_records_commands() {
        let mut runner = DryRunRunner::default();
        let _ = runner.run(&["apt-get".to_string(), "install".to_string()]);
        assert_eq!(runner.commands().len(), 1);
    }

    #[test]
    fn system_runner_runs_harmless_commands() {
        let mut runner = SystemRunner::direct();
        let ok = runner
            .run(&["true".to_string()])
            .expect("true should be available");
        assert!(ok.success());

        let failed = runner
            .run(&["false".to_string()])
            .expect("false should be available");
        assert!(!failed.success());
        assert!(!failed.failure_message().is_empty());
    }
}
