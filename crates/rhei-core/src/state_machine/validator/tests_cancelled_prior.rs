    // A step written behind a cancelled prior can never become ready, so
    // validation says so while the author is still writing it. Included into
    // `mod tests` in `state_machine/mod.rs`, so the indentation is the module's.

    // §FS-rhei-validate.4 §FS-rhei-states.1.4

    /// The review rounds of a supervised ticket: a custom abandonment terminal
    /// that declares `role: cancellation`, beside an ordinary terminal named
    /// like one, so the warning is shown to follow the role and not the name.
    fn cancellation_roles_machine() -> StateMachine {
        StateMachine::from_yaml_str(
            r#"
name: cancelled-prior
version: 1.0
states:
  review: { description: "review", initial: true }
  completed: { description: "done", final: true }
  cancelled: { description: "dropped by the supervisor", final: true }
  abandoned: { description: "abandoned", final: true, role: cancellation }
  dropped: { description: "an ordinary terminal", final: true }
transitions:
  - from: review
    to: completed
  - from: "*"
    to: cancelled
  - from: "*"
    to: abandoned
  - from: "*"
    to: dropped
"#,
        )
        .expect("states load")
    }

    fn cancelled_prior_warnings(input: &str) -> Vec<String> {
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &cancellation_roles_machine());
        assert!(!report.has_errors(), "must stay a warning: {:?}", report.errors);
        report.warnings.into_iter().filter(|w| w.contains("can never satisfy")).collect()
    }

    /// The report's own shape: round 2 of review and fix cancelled, round 3
    /// appended with both in its `**Prior:**`. One warning, naming the step,
    /// each cancelled prior, and the two remedies; the completed prior is not
    /// a cause and is not named.
    #[test]
    fn warns_when_an_open_task_waits_on_a_cancelled_prior() {
        let warnings = cancelled_prior_warnings(
            r#"# Rhei: Rounds
## Tasks

### Task 1: Implement the change
**State:** completed

### Task 2: Review round 2
**State:** cancelled
**Prior:** Task 1

### Task 3: Fix round 2
**State:** cancelled
**Prior:** Task 2

### Task 4: Review round 3
**State:** review
**Prior:** Task 3, Task 2, Task 1
"#,
        );

        assert_eq!(warnings.len(), 1, "one warning per step: {warnings:?}");
        let warning = &warnings[0];
        for needle in [
            "Task 4",
            "Task 3 (cancelled)",
            "Task 2 (cancelled)",
            "re-point",
            "**Prior:**",
            "**Consumes:**",
            "completed step",
            "cancel Task 4",
        ] {
            assert!(warning.contains(needle), "missing {needle:?} in:\n{warning}");
        }
        assert!(!warning.contains("Task 1 ("), "a completed prior is no cause:\n{warning}");
    }

    /// Cancellation is the prior's machine's classification, not its name: a
    /// `role: cancellation` terminal counts, an ordinary `dropped` does not.
    #[test]
    fn warns_for_a_prior_in_a_custom_cancellation_role_state() {
        let warnings = cancelled_prior_warnings(
            r#"# Rhei: Roles
## Tasks

### Task 1: Abandoned round
**State:** abandoned

### Task 2: Behind it
**State:** review
**Prior:** Task 1

### Task 3: Dropped round
**State:** dropped

### Task 4: Behind the ordinary terminal
**State:** review
**Prior:** Task 3
"#,
        );

        assert_eq!(warnings.len(), 1, "only the role counts: {warnings:?}");
        assert!(
            warnings[0].contains("Task 2") && warnings[0].contains("Task 1 (abandoned)"),
            "got:\n{}",
            warnings[0]
        );
    }

    /// A cancelled step waits on nothing, an open prior may yet finish, and a
    /// step already completed keeps the prior-order warning alone.
    #[test]
    fn does_not_warn_for_a_cancelled_step_an_open_prior_or_a_completed_step() {
        let input = r#"# Rhei: Quiet
## Tasks

### Task 1: Cancelled round
**State:** cancelled

### Task 2: Cancelled behind it
**State:** cancelled
**Prior:** Task 1

### Task 3: Open round
**State:** review

### Task 4: Behind an open prior
**State:** review
**Prior:** Task 3

### Task 5: Completed behind a cancelled prior
**State:** completed
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &cancellation_roles_machine());

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
        assert!(
            !report.warnings.iter().any(|w| w.contains("can never satisfy")),
            "unexpected cancelled-prior warning: {:?}",
            report.warnings
        );
        assert!(
            report.warnings.iter().any(|w| {
                w.contains("Task 5 is 'completed' but its prerequisites are unsatisfied")
            }),
            "the prior-order warning still covers the completed step: {:?}",
            report.warnings
        );
    }
