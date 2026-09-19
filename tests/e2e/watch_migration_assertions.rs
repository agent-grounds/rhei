//! Test-support contract for complete watch validation passes. §FS-rhei-validate.5

use std::path::Path;

use super::shell_quote;

const REVALIDATION_BANNER: &str = "--- change detected, revalidating ---";
const FAILED_REPORT_END: &str = "fix the errors above, then re-check with: rhei validate <plan>";
const SUCCESS_REPORT: &str = "Validation succeeded";

pub(super) fn expected_help(target: &Path) -> String {
    format!("help: rhei migrate export-priors {}", shell_quote(&target.display().to_string()))
}

/// Keep physical-line and exact-target checking common to ordinary and watch
/// diagnostics. The implementation step must leave this behavior unchanged.
pub(super) fn assert_complete_help_line(rendered: &str, target: &Path) {
    let expected = expected_help(target);
    assert!(
        rendered.lines().any(|line| {
            line.find("help: rhei migrate export-priors ")
                .is_some_and(|start| line[start..] == expected)
        }),
        "expected one complete physical help line {expected:?}; got:\n{rendered}"
    );
    assert_eq!(
        rendered.matches("rhei migrate export-priors").count(),
        1,
        "the recovery command should be rendered exactly once:\n{rendered}"
    );
}

/// Return complete pass bodies only after banner-based framing has closed
/// every report. Help text is content within a frame, never a frame boundary.
pub(super) fn complete_watch_passes(rendered: &str) -> Option<Vec<&str>> {
    let (started, after_start) = rendered.split_once('\n')?;
    if !started.starts_with("Watch mode started for '") {
        return None;
    }

    let passes: Vec<_> = after_start.split(REVALIDATION_BANNER).collect();
    if passes.len() < 2
        || passes[..passes.len() - 1].iter().any(|pass| !pass.contains(FAILED_REPORT_END))
        || !passes.last().is_some_and(|pass| pass.contains(SUCCESS_REPORT))
    {
        return None;
    }
    Some(passes)
}

/// Assertion seam for watch migration help.
///
/// The framing contract is already fixed, but this intentionally delegates to
/// the old aggregate assertion. Implementation may change only this delegation
/// to assert each failed pass separately.
pub(super) fn assert_watch_migration_help(rendered: &str, target: &Path) {
    let _passes = complete_watch_passes(rendered).unwrap_or_else(|| {
        panic!("watch output did not contain complete banner-framed passes:\n{rendered}")
    });
    assert_complete_help_line(rendered, target);
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::path::Path;

    use super::*;

    fn failed_report(target: &Path, commands: usize) -> String {
        let helps = std::iter::repeat_with(|| expected_help(target))
            .take(commands)
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "  x -- VALIDATION ERROR --\nTask plan.consumer is invalid\n{helps}\n\
             {FAILED_REPORT_END}\n"
        )
    }

    fn transcript(first: &str, second: &str) -> String {
        format!(
            "Watch mode started for 'plan.rhei.md' (states: 'states.yaml')\n\
             {first}{REVALIDATION_BANNER}\n{second}{REVALIDATION_BANNER}\n\
             {SUCCESS_REPORT}\n"
        )
    }

    fn rejected(rendered: &str, target: &Path) -> bool {
        catch_unwind(AssertUnwindSafe(|| assert_watch_migration_help(rendered, target))).is_err()
    }

    /// Separate failed passes may each carry one command. §FS-rhei-validate.5
    #[test]
    fn watch_help_accepts_two_complete_failed_passes() {
        let target = Path::new("plan.rhei.md");
        let report = failed_report(target, 1);
        assert_watch_migration_help(&transcript(&report, &report), target);
    }

    #[test]
    fn watch_help_rejects_two_commands_in_one_pass() {
        let target = Path::new("plan.rhei.md");
        let first = failed_report(target, 2);
        let second = failed_report(target, 1);
        assert!(rejected(&transcript(&first, &second), target));
    }

    #[test]
    fn watch_help_rejects_two_zero_distribution() {
        let target = Path::new("plan.rhei.md");
        let first = failed_report(target, 2);
        let second = failed_report(target, 0);
        assert!(rejected(&transcript(&first, &second), target));
    }

    #[test]
    fn watch_help_rejects_missing_help() {
        let target = Path::new("plan.rhei.md");
        let good = failed_report(target, 1);
        let missing = failed_report(target, 0);
        assert!(rejected(&transcript(&missing, &good), target));
    }

    #[test]
    fn watch_help_rejects_wrapped_help() {
        let target = Path::new("plan.rhei.md");
        let good = failed_report(target, 1);
        let wrapped = failed_report(target, 1).replace("export-priors ", "export-priors\n  ");
        assert!(rejected(&transcript(&wrapped, &good), target));
    }

    #[test]
    fn watch_help_rejects_wrong_target_help() {
        let target = Path::new("plan.rhei.md");
        let good = failed_report(target, 1);
        let wrong = failed_report(Path::new("other.rhei.md"), 1);
        assert!(rejected(&transcript(&wrong, &good), target));
    }

    #[test]
    fn watch_help_rejects_unframed_output() {
        let target = Path::new("plan.rhei.md");
        let report = failed_report(target, 1);
        assert!(rejected(&report, target));
    }

    #[test]
    fn watch_help_rejects_incomplete_reports() {
        let target = Path::new("plan.rhei.md");
        let report = failed_report(target, 1);
        let incomplete = format!(
            "Watch mode started for 'plan.rhei.md' (states: 'states.yaml')\n\
             {report}{REVALIDATION_BANNER}\npartial diagnostic"
        );
        assert!(rejected(&incomplete, target));
    }

    #[test]
    fn split_reads_wait_for_the_complete_final_report() {
        let target = Path::new("plan.rhei.md");
        let report = failed_report(target, 1);
        let complete = transcript(&report, &report);
        let split = complete.find(SUCCESS_REPORT).expect("success report");

        assert!(complete_watch_passes(&complete[..split]).is_none());
        assert_eq!(complete_watch_passes(&complete).expect("complete passes").len(), 3);
    }
}
