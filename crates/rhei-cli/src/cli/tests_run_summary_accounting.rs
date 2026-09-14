// What the run report's accounting strip and its task cost rows must count.
// `UsageReported` arrives repeatedly for one invocation as a streaming
// extractor observes more turns, and arrives after the slot is released, so
// both levels upsert on invocation id rather than appending. Included into
// `mod run_summary_tests` in `run_summary.rs`, so the indentation is the
// module's and `use super::*` is already in scope from `tests_run_summary.rs`.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.9

    /// One attempt identity assigned before its spawn. §FS-rhei-cost-accounting.3.7
    const INVOCATION: &str =
        "t1::work::codex::visit-1::run-fixture::move-0::attempt-1";

    /// A dimension an extractor measured, as one invocation reports it.
    fn measured(value: u64) -> rhei_tui::DimensionSummary {
        rhei_tui::DimensionSummary {
            value: Some(value),
            status: rhei_tui::DimensionStatus::Measured,
            missing_count: 0,
            measured_count: 1,
        }
    }

    fn unsupported() -> rhei_tui::DimensionSummary {
        rhei_tui::DimensionSummary {
            value: None,
            status: rhei_tui::DimensionStatus::Unsupported,
            missing_count: 1,
            measured_count: 0,
        }
    }

    fn presentation_summary(
        output_cache_write: rhei_tui::DimensionSummary,
    ) -> rhei_tui::AccountingRunSummary {
        rhei_tui::AccountingRunSummary {
            total: measured(1_150),
            input_total: measured(1_100),
            input_cached_read: measured(700),
            input_cache_write: measured(300),
            output_total: measured(50),
            output_cached_read: unsupported(),
            output_cache_write,
            cost_micro: None,
            priced_cost_micro: None,
            currency: Some("USD".to_string()),
            coverage: rhei_tui::UsageCoverage::Complete,
            pricing_status: rhei_tui::PricingStatus::Unpriced,
            invocation_count: 1,
            measured_invocation_count: 1,
            missing_invocation_count: 0,
        }
    }

    /// The run-level Markdown and TTY surfaces present every cache part beside
    /// its inclusive whole without changing the normalized total.
    // §FS-rhei-run-report.2.1
    #[test]
    fn accounting_presentation_orders_all_cache_dimensions_and_groups_the_tty_parts() {
        let strip = RunAccountingStrip {
            run: Some(presentation_summary(unsupported())),
            workspace: Some(rhei_tui::AccountingRunSummary {
                total: measured(2_300),
                ..presentation_summary(unsupported())
            }),
            source: AccountingSource::Rollup,
        };

        assert_eq!(
            strip.render_markdown(),
            "| Accounting (this run) | Value |\n\
             | --- | ---: |\n\
             | source | rollup |\n\
             | cost | unpriced |\n\
             | total tokens | 1.1k |\n\
             | input tokens (incl. cache) | 1.1k |\n\
             | input cache read | 700 |\n\
             | input cache write | 300 |\n\
             | output tokens (incl. cache) | 50 |\n\
             | output cache read | - |\n\
             | output cache write | - |\n\
             | coverage | Complete |\n\
             | workspace total tokens | 2.3k |\n\n"
        );
        assert_eq!(
            strip.render_console(),
            concat!(
                "  This run  unpriced · Total 1.1k · In 1.1k ",
                "(incl. cache: read 700, write 300) · Out 50 ",
                "(incl. cache: read -, write -) · Coverage Complete · via rollup\n",
                "  Workspace 2.3k tokens over its lifetime\n"
            )
        );
    }

    /// Missing and zero are different accounting facts and remain visibly
    /// different on both run-level surfaces.
    // §FS-rhei-run-report.2.1
    #[test]
    fn accounting_presentation_distinguishes_unavailable_from_measured_zero() {
        let strip = RunAccountingStrip {
            run: Some(presentation_summary(measured(0))),
            workspace: None,
            source: AccountingSource::RunEvents,
        };

        let markdown = strip.render_markdown();
        assert!(markdown.contains("| output cache read | - |"), "{markdown}");
        assert!(markdown.contains("| output cache write | 0 |"), "{markdown}");
        assert!(
            strip.render_console().contains("Out 50 (incl. cache: read -, write 0)"),
            "{}",
            strip.render_console()
        );
    }

    /// Task Costs uses the same seven token dimensions, in the same order,
    /// and exposes the fixture's priced cache write.
    // §FS-rhei-run-report.2.2
    #[test]
    fn accounting_presentation_task_costs_has_the_symmetric_cache_dimension_columns() {
        let sink = SummarySink::new();
        assign_slot(&sink, "1");
        let mut reported = usage(INVOCATION, 1_150, 50, 1_000);
        reported.input_total = measured(1_100);
        reported.input_cached_read = measured(700);
        reported.input_cache_write = measured(300);
        reported.output_cached_read = unsupported();
        reported.output_cache_write = measured(0);
        report_usage(&sink, "1", Some(0), reported);
        release_slot(&sink, "1");

        let rhei = rhei_core::parse(
            "# Rhei: Test Plan\n\n## Tasks\n\n### Task 1: Task 1\n**State:** completed\n",
        )
        .expect("plan parses");
        let rendered = RunSummaryReport::build(
            &rhei,
            &rhei_validator::MachineSet::single(machine()),
            &sink,
            test_stats(),
            "plan.rhei.md",
            &no_task_roots(),
        )
        .render_markdown();
        let task_costs = rendered
            .split_once("## Task Costs\n\n")
            .map(|(_, tail)| tail.split_once("\n\n").map_or(tail, |(table, _)| table))
            .expect("Task Costs section");

        assert_eq!(
            task_costs.lines().next(),
            Some(
                "| Task | Cost | Total | Input (incl. cache) | Input cache read | Input cache \
                 write | Output (incl. cache) | Output cache read | Output cache write | Coverage |"
            ),
            "{task_costs}"
        );
        assert!(
            task_costs.contains("| 1 | $0.00 | 1.1k | 1.1k | 700 | 300 | 50 | - | 0 | Complete |"),
            "{task_costs}"
        );
    }

    /// One invocation's usage. `total` and `output_total` carry separate numbers
    /// so a rollup that only gets `invocation_count` right cannot pass.
    fn usage(
        invocation_id: &str,
        total: u64,
        output_total: u64,
        cost_micro: u64,
    ) -> rhei_tui::UsageSummary {
        rhei_tui::UsageSummary {
            invocation_id: invocation_id.to_string(),
            state: "work".to_string(),
            agent: "codex".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-test".to_string()),
            total: measured(total),
            input_total: measured(total - output_total),
            input_cached_read: measured(0),
            input_cache_write: measured(0),
            output_total: measured(output_total),
            output_cached_read: measured(0),
            output_cache_write: measured(0),
            cost_micro: Some(cost_micro),
            priced_cost_micro: Some(cost_micro),
            currency: Some("USD".to_string()),
            coverage: rhei_tui::UsageCoverage::Complete,
            status: rhei_tui::UsageStatus::Measured,
            pricing_status: rhei_tui::PricingStatus::Priced,
        }
    }

    fn report_usage(
        sink: &SummarySink,
        task: &str,
        slot: Option<u16>,
        usage: rhei_tui::UsageSummary,
    ) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::UsageReported {
            slot,
            task: task.to_string(),
            invocation_id: usage.invocation_id.clone(),
            report: rhei_tui::UsageReport::Final,
            usage,
        });
    }

    fn assign_slot(sink: &SummarySink, task: &str) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::SlotAssigned {
            slot: 0,
            task: task.to_string(),
            from: "work".to_string(),
            to: "completed".to_string(),
            agent: Some("codex".to_string()),
            template_context: None,
            log_path: std::path::PathBuf::from("runtime/logs/x.log"),
            started_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
        });
    }

    fn release_slot(sink: &SummarySink, task: &str) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::SlotReleased {
            slot: 0,
            task: task.to_string(),
            from: "work".to_string(),
            to: "completed".to_string(),
            log_path: std::path::PathBuf::from("runtime/logs/x.log"),
            outcome: rhei_tui::TaskOutcome::Completed,
            finished_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
            exit_code: Some(0),
            duration_ms: 1_200,
        });
    }

    /// The task's own accounting, as the console task row and the report's task
    /// table read it out of the snapshot.
    fn task_accounting(sink: &SummarySink, task: &str) -> rhei_tui::AccountingRunSummary {
        sink.snapshot()
            .get(task)
            .and_then(|activity| activity.accounting.clone())
            .expect("the task carries direct accounting")
    }

    /// The ticket's case. One invocation reported twice, with the slot released
    /// between the reports, is one invocation on both surfaces — not two, and
    /// not twice the tokens. Appending the second report made an aborted run's
    /// cost strip and every run's task cost row read double.
    // §FS-rhei-cost-accounting.3.7 §FS-rhei-cost-accounting.9
    #[test]
fn attempt_identity_repeated_usage_for_one_attempt_counts_once() {
        let sink = SummarySink::new();
        assign_slot(&sink, "t1");
        report_usage(&sink, "t1", Some(0), usage(INVOCATION, 1_280_000, 96_000, 21_000));
        release_slot(&sink, "t1");
        // The same invocation reports again after its slot is gone.
        report_usage(&sink, "t1", None, usage(INVOCATION, 1_280_000, 96_000, 21_000));

        let run = sink.accounting().run.expect("the run carries fallback accounting");
        assert_eq!(run.invocation_count, 1, "run rollup: {run:?}");
        assert_eq!(run.total.value, Some(1_280_000), "run rollup: {run:?}");
        assert_eq!(run.output_total.value, Some(96_000), "run rollup: {run:?}");
        assert_eq!(run.cost_micro, Some(21_000), "run rollup: {run:?}");

        let task = task_accounting(&sink, "t1");
        assert_eq!(task.invocation_count, 1, "task rollup: {task:?}");
        assert_eq!(task.total.value, Some(1_280_000), "task rollup: {task:?}");
        assert_eq!(task.output_total.value, Some(96_000), "task rollup: {task:?}");
        assert_eq!(task.cost_micro, Some(21_000), "task rollup: {task:?}");
    }

    /// A streaming extractor's later report is the truth: it replaces what the
    /// same invocation reported before, rather than being added to it or
    /// dropped in favour of it. Two identical reports cannot tell those apart,
    /// so the second one here is larger than the first.
    // §FS-rhei-cost-accounting.9
    #[test]
    fn a_later_usage_report_replaces_the_earlier_one_for_the_same_invocation() {
        let sink = SummarySink::new();
        assign_slot(&sink, "t1");
        report_usage(&sink, "t1", Some(0), usage(INVOCATION, 400_000, 30_000, 7_000));
        release_slot(&sink, "t1");
        report_usage(&sink, "t1", None, usage(INVOCATION, 1_280_000, 96_000, 21_000));

        for (level, rollup) in [
            ("run", sink.accounting().run.expect("the run carries fallback accounting")),
            ("task", task_accounting(&sink, "t1")),
        ] {
            assert_eq!(rollup.invocation_count, 1, "{level} rollup: {rollup:?}");
            // The later report's numbers, not the earlier ones (400_000) and not
            // their sum (1_680_000).
            assert_eq!(rollup.total.value, Some(1_280_000), "{level} rollup: {rollup:?}");
            assert_eq!(rollup.output_total.value, Some(96_000), "{level} rollup: {rollup:?}");
            assert_eq!(rollup.cost_micro, Some(21_000), "{level} rollup: {rollup:?}");
        }
    }

    /// The guard on the upsert: two genuinely different invocations still count
    /// as two and still add up. Keying on invocation id must not collapse the
    /// case that was always right.
    // §FS-rhei-cost-accounting.9
    #[test]
fn attempt_identity_distinct_attempts_still_roll_up_to_their_sum() {
        let sink = SummarySink::new();
        assign_slot(&sink, "t1");
        report_usage(&sink, "t1", Some(0), usage(INVOCATION, 400_000, 30_000, 7_000));
        release_slot(&sink, "t1");
        report_usage(
            &sink,
            "t1",
            Some(0),
            usage(
                "t1::work::codex::visit-1::run-fixture::move-0::attempt-2",
                1_280_000,
                96_000,
                21_000,
            ),
        );

        for (level, rollup) in [
            ("run", sink.accounting().run.expect("the run carries fallback accounting")),
            ("task", task_accounting(&sink, "t1")),
        ] {
            assert_eq!(rollup.invocation_count, 2, "{level} rollup: {rollup:?}");
            assert_eq!(rollup.total.value, Some(1_680_000), "{level} rollup: {rollup:?}");
            assert_eq!(rollup.output_total.value, Some(126_000), "{level} rollup: {rollup:?}");
            assert_eq!(rollup.cost_micro, Some(28_000), "{level} rollup: {rollup:?}");
        }
    }
