// The built-in machine's structural manual guard must yield to this run's
// actual work, while independent blockers retain their existing precedence.
// §FS-rhei-run-report.3.1 §FS-rhei-run.3

    fn pending_cause(worked: bool, missing: bool, interrupted: bool, metadata: &str) -> HaltCause {
        let rhei = rhei_core::parse(&format!(
            "# Rhei: Manual\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n{metadata}\n"
        )).expect("built-in-machine plan");
        let machines = rhei_validator::MachineSet::single(machine());
        classify_halted_tasks(
            &rhei,
            &machines,
            &None,
            &|_| worked,
            &|_, _| missing.then(|| UnwrittenOutputs {
                entries: vec!["result (runtime/results/1.md)".to_string()],
                exit_code: Some(0),
                configuration_fallbacks: Vec::new(),
            }),
            &|_| interrupted,
            "plan.rhei.md",
            &no_task_roots(),
        ).remove(0).1
    }

    #[test]
    fn issue_597_pending_work_with_missing_outputs_is_missing_outputs() {
        let cause = pending_cause(true, true, false, "");
        assert!(matches!(&cause, HaltCause::MissingOutputs { entries, exit_code: Some(0), .. }
            if entries == &["result (runtime/results/1.md)".to_string()]), "{cause:?}");
    }

    #[test]
    fn issue_597_pending_work_without_missing_outputs_is_stalled() {
        let cause = pending_cause(true, false, false, "");
        assert!(matches!(cause, HaltCause::Stalled), "{cause:?}");
        let (reason, next) = cause.describe("1", "pending");
        assert!(reason.contains("stalled"), "{reason}");
        assert!(next.contains("inspect logs"), "{next}");
    }

    #[test]
    fn issue_597_pending_interruption_outranks_missing_outputs_and_manual_shape() {
        for missing in [false, true] {
            let cause = pending_cause(true, missing, true, "");
            assert!(matches!(cause, HaltCause::Interrupted), "{cause:?}");
            let (reason, next) = cause.describe("1", "pending");
            assert!(reason.contains("interrupted"), "{reason}");
            assert_eq!(next, "re-run to continue");
        }
    }

    #[test]
    fn issue_597_pending_work_keeps_independent_claim_and_prior_blockers() {
        for (metadata, claimed) in [
            ("**Assignee:** someone", true),
            ("**Prior:** 2\n\n### Task 2: Prior\n**State:** pending", false),
        ] {
            for interrupted in [false, true] {
                let cause = pending_cause(true, true, interrupted, metadata);
                if claimed {
                    assert!(matches!(cause, HaltCause::Claimed { .. }), "{cause:?}");
                } else {
                    assert!(matches!(cause, HaltCause::BlockedByPrior { .. }), "{cause:?}");
                }
            }
        }
        assert!(matches!(pending_cause(false, false, false, ""), HaltCause::ManualOnly { .. }));
    }
