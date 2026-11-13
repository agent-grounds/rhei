// Unit coverage for the pieces of `rhei summary` that a black-box test can
// only reach through a whole fixture. §FS-rhei-summary

fn summary_machine() -> rhei_validator::StateMachine {
    serde_yaml::from_str(
        r#"name: supervised-ticket-fix
version: 1
states:
  supervising:
    initial: true
  implement: {}
  completed:
    final: true
  cancelled:
    final: true
"#,
    )
    .expect("summary test machine parses")
}

fn summary_plan(states: &[&str]) -> rhei_core::ast::Rhei {
    let mut text = String::from("# Rhei: Tally\n\n## Tasks\n");
    for (index, state) in states.iter().enumerate() {
        text.push_str(&format!(
            "\n### Task {}: Step {}\n**State:** {state}\n",
            index + 1,
            index + 1
        ));
    }
    rhei_core::parse(&text).expect("summary test plan parses")
}

fn summary_record(task_id: &str, visit: u64, started_at: &str, ended_at: &str) -> AccountingInvocationRecord {
    AccountingInvocationRecord {
        started_at: started_at.to_string(),
        ended_at: ended_at.to_string(),
        task_id: task_id.to_string(),
        visit,
        ..accounting_test_record()
    }
}

fn summary_dimension(value: Option<u64>) -> rhei_tui::DimensionSummary {
    rhei_tui::DimensionSummary {
        value,
        status: if value.is_some() {
            rhei_tui::DimensionStatus::Measured
        } else {
            rhei_tui::DimensionStatus::Unsupported
        },
        missing_count: u64::from(value.is_none()),
        measured_count: u64::from(value.is_some()),
    }
}

/// The aggregate table uses the run report's ordered presentation, preserves
/// the inclusive total, and distinguishes unavailable from measured zero.
// §FS-rhei-summary.2.3
#[test]
fn accounting_presentation_summary_uses_the_symmetric_cache_dimension_rows() {
    let inspection = CostInspection::from_records(
        Some(rhei_tui::AccountingRunSummary {
            total: summary_dimension(Some(1_150)),
            input_total: summary_dimension(Some(1_100)),
            input_cached_read: summary_dimension(Some(700)),
            input_cache_write: summary_dimension(Some(300)),
            output_total: summary_dimension(Some(50)),
            output_cached_read: summary_dimension(None),
            output_cache_write: summary_dimension(Some(0)),
            cost_micro: None,
            priced_cost_micro: None,
            currency: Some("USD".to_string()),
            coverage: rhei_tui::UsageCoverage::Complete,
            pricing_status: rhei_tui::PricingStatus::Unpriced,
            invocation_count: 1,
            measured_invocation_count: 1,
            missing_invocation_count: 0,
            unrecorded_agent_invocation_count: 0,
        }),
        Vec::new(),
    );

    assert_eq!(
        summary_accounting(&inspection),
        "| Accounting | Value |\n\
         | --- | ---: |\n\
         | total tokens | 1.1k |\n\
         | input tokens (incl. cache) | 1.1k |\n\
         | input cache read | 700 |\n\
         | input cache write | 300 |\n\
         | output tokens (incl. cache) | 50 |\n\
         | output cache read | - |\n\
         | output cache write | 0 |\n\
         | coverage | Complete |\n"
    );
}

#[test]
fn task_tally_counts_terminals_in_machine_order_then_the_remainder() {
    // §FS-rhei-summary.2.1: terminal states in declaration order, with the
    // non-terminal remainder appended as `N in progress`.
    let machine = summary_machine();
    let plan = summary_plan(&["cancelled", "completed", "completed", "implement"]);
    assert_eq!(summary_task_tally(&plan, &machine, &None), "2 tasks completed, 1 cancelled, 1 in progress");
}

#[test]
fn task_tally_stays_singular_and_names_an_empty_plan() {
    let machine = summary_machine();
    assert_eq!(summary_task_tally(&summary_plan(&["completed"]), &machine, &None), "1 task completed");
    assert_eq!(summary_task_tally(&summary_plan(&[]), &machine, &None), "no tasks");
}

#[test]
fn a_visit_is_printed_only_where_a_task_has_more_than_one_record() {
    // §FS-rhei-summary.2.2: a repeated supervisor visit is distinguishable and
    // a one-shot step stays clean.
    let inspection = CostInspection::from_records(
        None,
        vec![
            summary_record("1", 1, "2026-05-20T10:00:00Z", "2026-05-20T10:02:32Z"),
            summary_record("2", 1, "2026-05-20T10:03:00Z", "2026-05-20T10:21:04Z"),
            summary_record("1", 2, "2026-05-20T10:22:00Z", "2026-05-20T10:22:45Z"),
        ],
    );
    let steps = summary_steps(&inspection);
    assert!(steps.contains("1. `1` work (visit 1) —"), "got:\n{steps}");
    assert!(steps.contains("2. `2` work — "), "got:\n{steps}");
    assert!(steps.contains("3. `1` work (visit 2) —"), "got:\n{steps}");
}

#[test]
fn a_step_duration_is_omitted_when_a_timestamp_will_not_parse() {
    // §FS-rhei-summary.2.2: an unparseable timestamp yields no duration rather
    // than a guessed one.
    let good = summary_record("1", 1, "2026-05-20T10:00:00Z", "2026-05-20T10:02:32Z");
    assert_eq!(summary_step_duration(&good).as_deref(), Some("2m32s"));
    let bad = summary_record("1", 1, "not-a-timestamp", "2026-05-20T10:02:32Z");
    assert_eq!(summary_step_duration(&bad), None);
    let backwards = summary_record("1", 1, "2026-05-20T10:02:32Z", "2026-05-20T10:00:00Z");
    assert_eq!(summary_step_duration(&backwards), None);
}

#[test]
fn an_unmeasured_record_contributes_no_token_clause() {
    // §FS-rhei-summary.4: an unmeasured record contributes no token line.
    let books = ReachablePriceBooks::builtin_only();
    let record = summary_record("1", 1, "2026-05-20T10:00:00Z", "2026-05-20T10:02:32Z");
    assert_eq!(summary_step_tokens(&record, &books), None);

    let mut measured = record;
    measured.tokens.input.total = AccountingTokenDimension::measured(41_200);
    measured.tokens.output.total = AccountingTokenDimension::measured(3_800);
    assert_eq!(summary_step_tokens(&measured, &books).as_deref(), Some("41.2k in / 3.8k out"));
}

#[test]
fn an_empty_accounting_store_still_renders_a_lead_line_and_the_unmeasured_line() {
    // §FS-rhei-summary.5: a freshly instantiated workspace is summarizable.
    let inspection = CostInspection::from_records(None, Vec::new());
    let rendered =
        render_summary(&summary_plan(&["implement"]), &summary_machine(), &inspection, &None, false);
    assert_eq!(
        rendered,
        "`supervised-ticket-fix` workflow: 0 agent invocations across 0 models; \
         1 task in progress.\n\nToken accounting was not measured for this run.\n"
    );
}

/// An agent spawn of task `task`, state `work`, that no record answers for.
fn summary_spawn(task: &str, worker: &str, started: &str, ended: &str) -> UnrecordedSpawn {
    UnrecordedSpawn {
        task: task.to_string(),
        state: "work".to_string(),
        moves: 1,
        attempt: 1,
        spawn: format!("task-{task}-work-{started}.json"),
        worker: worker.to_string(),
        started: started.to_string(),
        ended: ended.to_string(),
        killed: false,
    }
}

/// A killed spawn's `ended` is when the next run learned of the kill, so it has
/// no duration, however far apart its timestamps are. §FS-rhei-summary.2.2
#[test]
fn a_killed_spawn_has_no_elapsed_time() {
    let ended = summary_spawn("1", "mock", "2026-05-20T10:00:00Z", "2026-05-20T10:05:00Z");
    assert_eq!(ended.elapsed_ms(), Some(300_000));
    assert_eq!(UnrecordedSpawn { killed: true, ..ended }.elapsed_ms(), None);
}

#[test]
fn a_folded_entry_sums_its_durations_and_omits_the_sum_when_one_attempt_has_none() {
    // §FS-rhei-summary.2.2.1: the duration is the sum of every attempt's own,
    // so one attempt without one leaves the entry with none, not a partial sum.
    let timed = CostInspection::from_records(
        None,
        vec![
            summary_record("1", 2, "2026-05-20T10:00:00Z", "2026-05-20T10:01:00Z"),
            summary_record("1", 2, "2026-05-20T10:02:00Z", "2026-05-20T10:02:30Z"),
            summary_record("1", 2, "2026-05-20T10:03:00Z", "2026-05-20T10:03:45Z"),
        ],
    );
    assert_eq!(summary_steps(&timed), "1. `1` work (visit 2) ×3 — codex, openai/gpt-test — 2m15s\n");

    let untimed = CostInspection::from_records(
        None,
        vec![
            summary_record("1", 2, "2026-05-20T10:00:00Z", "2026-05-20T10:01:00Z"),
            summary_record("1", 2, "not-a-timestamp", "2026-05-20T10:02:30Z"),
            summary_record("1", 2, "2026-05-20T10:03:00Z", "2026-05-20T10:03:45Z"),
        ],
    );
    assert_eq!(summary_steps(&untimed), "1. `1` work (visit 2) ×3 — codex, openai/gpt-test\n");
}

#[test]
fn a_folded_entry_prints_a_token_side_only_when_every_attempt_measured_it() {
    // §FS-rhei-summary.2.2.1: each side is the sum of what every record would
    // print for it, and a side one record did not measure is left out.
    let attempt = |input: Option<u64>, output: Option<u64>| {
        let mut record = summary_record("1", 1, "2026-05-20T10:00:00Z", "2026-05-20T10:01:00Z");
        if let Some(value) = input {
            record.tokens.input.total = AccountingTokenDimension::measured(value);
        }
        if let Some(value) = output {
            record.tokens.output.total = AccountingTokenDimension::measured(value);
        }
        record
    };
    let both = CostInspection::from_records(
        None,
        vec![attempt(Some(41_200), Some(3_800)), attempt(Some(1_000), Some(200))],
    );
    assert_eq!(
        summary_steps(&both),
        "1. `1` work (visit 1) ×2 — codex, openai/gpt-test — 2m00s — 42.2k in / 4.0k out\n"
    );

    let one_side =
        CostInspection::from_records(None, vec![attempt(Some(41_200), Some(3_800)), attempt(Some(1_000), None)]);
    assert_eq!(summary_steps(&one_side), "1. `1` work (visit 1) ×2 — codex, openai/gpt-test — 2m00s — 42.2k in\n");

    let neither = CostInspection::from_records(None, vec![attempt(Some(41_200), Some(3_800)), attempt(None, None)]);
    assert_eq!(summary_steps(&neither), "1. `1` work (visit 1) ×2 — codex, openai/gpt-test — 2m00s\n");
}

#[test]
fn consecutive_unrecorded_spawns_fold_by_task_state_and_worker_and_never_with_a_record() {
    // §FS-rhei-summary.2.2.1: spawns fold on task, state and worker; a record
    // of the same task, state and agent beside them stays its own entry.
    let mut inspection = CostInspection::from_records(
        None,
        vec![summary_record("1", 1, "2026-05-20T10:00:00Z", "2026-05-20T10:01:00Z")],
    );
    inspection.unrecorded = vec![
        summary_spawn("1", "codex", "2026-05-20T10:02:00Z", "2026-05-20T10:02:01Z"),
        summary_spawn("1", "codex", "2026-05-20T10:03:00Z", "2026-05-20T10:03:02Z"),
        summary_spawn("1", "cdx", "2026-05-20T10:04:00Z", "2026-05-20T10:04:01Z"),
    ];
    assert_eq!(
        summary_steps(&inspection),
        "1. `1` work — codex, openai/gpt-test — 1m00s\n\
         2. `1` work ×2 — codex — 3.0s — no accounting record\n\
         3. `1` work — cdx — 1.0s — no accounting record\n"
    );
}
