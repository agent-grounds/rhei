// Lifetime reporting uses the account and one effective remedy on both producers.
// §FS-rhei-budgets.2.3 §FS-rhei-budgets.8 §FS-rhei-run-json.2.1

mod lifetime_reporting {
    use super::*;
    use crate::rhei_tui::RunEvent;
    use rhei_core::budget::{Audit, Counter, Exhaustion, SettingsFiles};
    use std::time::SystemTime;

    fn bounds(project_max: bool) -> CountBounds {
        let files = SettingsFiles {
            machine: Some(PathBuf::from("machine/settings.json")),
            project: Some(PathBuf::from("project/settings.json")),
        };
        let resolve = |key, value| Bound::resolve(key, value, None, None).with_files(files.clone());
        CountBounds {
            travel: Bound::resolve_delegated("transition_limit", 3, Some((8, BoundSource::Plan)))
                .with_files(files.clone()),
            per_day: resolve("invocations_per_day", 50),
            lifetime_max: Bound::resolve(
                "invocation_lifetime_max",
                10,
                Some(10),
                project_max.then_some((2, BoundSource::Project)),
            )
            .with_files(files.clone()),
            spend: Bound::resolve_money("spend_per_day", 400_000_000, None, None).with_files(files),
        }
    }

    fn journal(lifetime: bool) -> (tempfile::TempDir, Journal) {
        let dir = tempfile::tempdir().unwrap();
        let audit = Audit {
            actor: "test".into(),
            written_at: "2026-11-07T12:00:00Z".into(),
            reason: "issue579".into(),
            argv: vec![],
        };
        let (_, mut journal) = Account::establish(dir.path(), &audit).unwrap();
        if lifetime {
            journal.adjust(2, &audit).unwrap();
        }
        (dir, journal)
    }

    fn refusal(dimension: Dimension, contract: Contract, bound: u64, day: &str) -> BudgetError {
        BudgetError {
            reason_code: "invocation_exhausted".into(),
            message: "spent".into(),
            exhaustion: Some(Box::new(Exhaustion {
                dimension,
                subject: "project".into(),
                counter: Counter { consumed: bound, reserved: 0 },
                bound,
                contract,
                day: day.into(),
                currency: dimension.is_money().then(|| "USD".into()),
            })),
        }
    }

    // Exercise existing callable producers, without requiring a not-yet-written resolver.
    // §FS-rhei-budgets.8
    fn remedy_table(include_lifetime: bool) {
        let (_window_dir, window) = journal(false);
        let (_lifetime_dir, lifetime) = journal(true);
        let bounds = bounds(false);
        let project_bounds = self::bounds(true);
        let _target = enter_help_target(HelpTarget::naming(Path::new("sub dir/plan's.rhei.md")));
        let command = format!(
            "rhei budget adjust {} --invocations <N> --reason <TEXT>",
            shell_quote("sub dir/plan's.rhei.md")
        );
        let adjust = format!("run `{command}`");
        let renews = window.renewal_instant().unwrap().unwrap();
        let mut cases = vec![
            (
                "travel delegated",
                &bounds,
                &window,
                Dimension::Travel,
                Contract::Window,
                3,
                bounds.travel.remedy(),
                None,
            ),
            (
                "window renewal",
                &bounds,
                &window,
                Dimension::Invocations,
                Contract::Window,
                50,
                format!("wait: the window renews at {renews}"),
                Some(renews.clone()),
            ),
            (
                "window without instant",
                &bounds,
                &lifetime,
                Dimension::Invocations,
                Contract::Window,
                50,
                bounds.per_day.remedy(),
                None,
            ),
            (
                "spend on window",
                &bounds,
                &window,
                Dimension::Spend,
                Contract::Window,
                400_000_000,
                format!("wait: the window renews at {renews}"),
                Some(renews.clone()),
            ),
            (
                "spend on lifetime",
                &bounds,
                &lifetime,
                Dimension::Spend,
                Contract::Lifetime { allowance: 2 },
                400_000_000,
                format!("wait: the window renews at {renews}"),
                Some(renews.clone()),
            ),
        ];
        if include_lifetime {
            for (name, b, allowance) in [
                ("lifetime below ceiling", &bounds, 2),
                ("lifetime equal machine ceiling", &bounds, 10),
                ("lifetime above machine ceiling", &bounds, 12),
                ("lifetime equal project ceiling", &project_bounds, 2),
                ("lifetime above project ceiling", &project_bounds, 3),
            ] {
                let remedy = if allowance < b.lifetime_max.effective {
                    adjust.clone()
                } else {
                    format!("{} above {allowance}, then {adjust}", b.lifetime_max.remedy())
                };
                cases.push((
                    name,
                    b,
                    &lifetime,
                    Dimension::Invocations,
                    Contract::Lifetime { allowance },
                    allowance,
                    remedy,
                    None,
                ));
            }
        }
        let mut failures = Vec::new();
        for (name, bounds, journal, dimension, contract, bound, expected, instant) in cases {
            let error = refusal(dimension, contract, bound, window.day());
            let text = budget_halt_text(&error, bounds, journal);
            let event = budget_halt_event(&error, bounds, journal, "plan.2").unwrap();
            let RunEvent::BudgetHalt { remedy, renews_at, .. } = event else {
                panic!("halt event")
            };
            let row_matches = match &instant {
                Some(at) => text.lines().any(|row| row.trim() == format!("renews at:   {at}")),
                None => text.lines().any(|row| row.trim() == format!("to raise it: {expected}")),
            };
            if !row_matches || remedy != expected || renews_at != instant {
                failures.push(format!(
                    "{name}: expected {expected:?}; record={remedy:?}; stderr={text}"
                ));
            }
        }
        assert!(failures.is_empty(), "shared remedy table:\n{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "red until #579: stderr and record share the effective lifetime remedy; implement removes this"]
    fn issue579_shared_remedy_table() {
        remedy_table(true);
    }

    #[test]
    fn issue579_window_travel_and_spend_remedy_preservation() {
        remedy_table(false);
    }

    // Additive account strings already round-trip in schema 1; a passing compatibility control.
    // §FS-rhei-run-json.2.1 §FS-rhei-run-json.2.2
    #[test]
    fn issue579_account_schema_one_roundtrip() {
        assert_eq!(rhei_tui::SCHEMA_VERSION, 1);
        let (_dir, journal) = journal(true);
        let mut reports = budget_reports(&journal, "ticket", &bounds(false)).unwrap();
        reports[0].value_source = "account".into();
        let event = RunEvent::BudgetSnapshot { bounds: reports };
        let at = SystemTime::now();
        let encoded = rhei_tui::encode_event(Some(1), &event, at, None);
        let decoded = rhei_tui::decode_event(&encoded.to_string()).unwrap();
        assert_eq!(rhei_tui::encode_event(Some(1), &decoded.event, at, None), encoded);
        assert_eq!(encoded["bounds"][0]["value_source"], "account");
    }

    // §FS-rhei-budgets.2.3 §FS-rhei-budgets.9
    #[test]
    #[ignore = "red until #579: account source renders on invocation snapshots and halt text; implement removes this"]
    fn issue579_account_source_rendering() {
        let (_dir, journal) = journal(true);
        let bounds = bounds(false);
        let reports = budget_reports(&journal, "ticket", &bounds).unwrap();
        assert_eq!(reports[0].value_source, "account", "lifetime snapshot provenance");
        assert!(rhei_tui::bound_journal_line(&reports[0]).contains("(account)"));
        let error =
            refusal(Dimension::Invocations, Contract::Lifetime { allowance: 2 }, 2, journal.day());
        assert!(budget_halt_text(&error, &bounds, &journal).contains("2 (account)"));
    }
}
