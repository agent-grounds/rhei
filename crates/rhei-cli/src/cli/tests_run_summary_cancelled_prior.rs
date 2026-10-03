// The halt row for a ticket whose `**Prior:**` is cancelled: the one prior that
// can never finish, so "finish the prior first" is the one advice it must not
// get. Included into `mod run_summary_tests` in `run_summary.rs`, so the
// indentation is the module's and `use super::*` is already in scope from
// `tests_run_summary.rs`.

// §FS-rhei-run-report.3.1 §FS-rhei-states.1.4

    /// The review rounds of a supervised ticket, with a custom abandonment
    /// terminal beside the reserved name so the reading follows the role.
    fn cancelled_prior_report(tasks: &str) -> RunSummaryReport {
        let rhei = rhei_core::parse(&format!("# Rhei: Rounds\n\n## Tasks\n\n{tasks}"))
            .expect("plan parses");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"name: rounds
version: 1
states:
  review: { description: Review, initial: true, program: "./review.sh" }
  completed: { description: Done, final: true }
  cancelled: { description: Dropped, final: true }
  abandoned: { description: Abandoned, final: true, role: cancellation }
transitions:
  - { from: review, to: completed }
  - { from: "*", to: cancelled }
  - { from: "*", to: abandoned }
"#,
        )
        .expect("valid state machine");
        RunSummaryReport::build(
            &rhei,
            &rhei_validator::MachineSet::single(machine),
            &SummarySink::new(),
            test_stats(),
            "plan.rhei.md",
            &no_task_roots(),
        )
    }

    /// The row's blocker says the prior is cancelled and can never satisfy the
    /// ticket; its next action names re-pointing `**Prior:**` and
    /// `**Consumes:**` at a completed step, or cancelling the ticket.
    fn assert_names_the_cancelled_prior(row: &AttentionRow, prior: &str, id: &str) {
        assert!(
            row.reason.contains(prior) && row.reason.contains("can never satisfy"),
            "the blocker names {prior} as cancelled for good: {:?}",
            row.reason
        );
        assert!(!row.next.contains("finish the prior"), "a cancelled prior cannot finish: {:?}", row.next);
        for needle in ["re-point", "**Prior:**", "**Consumes:**", "completed step", &format!("cancel Task {id}")] {
            assert!(row.next.contains(needle), "missing {needle:?} in next action: {:?}", row.next);
        }
        assert!(!row.is_gate, "it is still prior-blocked work a human must act on");
    }

    /// agent-grounds/rhei#409: round 2 of review and fix cancelled, round 3
    /// appended behind them. The console summary and the durable report both
    /// read off this row, and both told the operator to finish Task 3.
    #[test]
    fn a_cancelled_prior_is_named_as_cancelled_not_as_one_to_finish() {
        let report = cancelled_prior_report(
            "### Task 1: Implement the change\n**State:** completed\n\n\
             ### Task 2: Review round 2\n**State:** cancelled\n**Prior:** 1\n\n\
             ### Task 3: Fix round 2\n**State:** cancelled\n**Prior:** 2\n\n\
             ### Task 4: Review round 3\n**State:** review\n**Prior:** 3, 2, 1\n",
        );

        let row = report.attention.iter().find(|row| row.id == "4").expect("Task 4 is an action item");
        assert_names_the_cancelled_prior(row, "Task 3 (cancelled)", "4");

        let markdown = report.render_markdown();
        assert!(!markdown.contains("finish the prior first"), "{markdown}");
        assert!(markdown.contains("| could not advance | 1 |"), "{markdown}");
    }

    /// A live prior may yet finish; a cancelled one never will, so it is the
    /// cause the row names, whatever order `**Prior:**` lists them in.
    #[test]
    fn a_cancelled_prior_outranks_a_live_one_listed_before_it() {
        let report = cancelled_prior_report(
            "### Task 1: Still reviewing\n**State:** review\n\n\
             ### Task 2: Dropped round\n**State:** cancelled\n\n\
             ### Task 3: Behind both\n**State:** review\n**Prior:** 1, 2\n",
        );

        let row = report.attention.iter().find(|row| row.id == "3").expect("Task 3 is an action item");
        assert_names_the_cancelled_prior(row, "Task 2 (cancelled)", "3");
    }

    /// `role: cancellation` is what makes a prior cancelled, not its name.
    #[test]
    fn a_prior_in_a_custom_cancellation_role_state_reads_as_cancelled() {
        let report = cancelled_prior_report(
            "### Task 1: Abandoned round\n**State:** abandoned\n\n\
             ### Task 2: Behind it\n**State:** review\n**Prior:** 1\n",
        );

        let row = report.attention.iter().find(|row| row.id == "2").expect("Task 2 is an action item");
        assert_names_the_cancelled_prior(row, "Task 1 (abandoned)", "2");
    }
