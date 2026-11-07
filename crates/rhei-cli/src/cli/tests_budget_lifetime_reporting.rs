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
        BudgetError::exhausted(
            "invocation_exhausted",
            "spent",
            Exhaustion {
                dimension,
                subject: "project".into(),
                counter: Counter { consumed: bound, reserved: 0 },
                bound,
                contract,
                day: day.into(),
                currency: dimension.is_money().then(|| "USD".into()),
            },
        )
    }

    // Pin the resolved value and its rendering by both producers for every halt kind.
    // §FS-rhei-budgets.8
    fn remedy_table(include_lifetime: bool) {
        let (_window_dir, window) = journal(false);
        let (_lifetime_dir, lifetime) = journal(true);
        let bounds = bounds(false);
        let project_bounds = self::bounds(true);
        let maxima = |machine, project: Option<u64>| {
            let mut b = bounds.clone();
            b.lifetime_max = Bound::resolve(
                "invocation_lifetime_max",
                10,
                Some(machine),
                project.map(|value| (value, BoundSource::Project)),
            )
            .with_files(bounds.lifetime_max.files.clone());
            b
        };
        let equal_bounds = maxima(1, Some(1));
        let lower_project = maxima(2, Some(1));
        let lower_machine = maxima(1, Some(2));
        let higher_project = maxima(1, Some(10));
        let mut same_file = equal_bounds.clone();
        same_file.lifetime_max.files.project = same_file.lifetime_max.files.machine.clone();
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
                Remedy::Raise,
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
                Remedy::Renews(renews.clone()),
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
                Remedy::Raise,
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
                Remedy::Renews(renews.clone()),
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
                Remedy::Renews(renews.clone()),
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
                let resolved = if allowance < b.lifetime_max.effective {
                    Remedy::Adjust(command.clone())
                } else {
                    Remedy::RaiseThenAdjust {
                        ceilings: vec![b.lifetime_max.remedy()],
                        allowance,
                        command: command.clone(),
                    }
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
                    resolved,
                ));
            }
            // Every independent constraint at/below the allowance must change,
            // keeping the own-remedy edit first and avoiding unnecessary edits.
            // §FS-rhei-budgets.8
            for (name, b, allowance, files) in [
                ("equal ceilings", &equal_bounds, 1, vec!["machine", "project"]),
                ("above both equal ceilings", &equal_bounds, 3, vec!["machine", "project"]),
                ("above unequal project first", &lower_project, 3, vec!["project", "machine"]),
                ("above unequal machine first", &lower_machine, 3, vec!["machine", "project"]),
                ("project already permits increase", &higher_project, 2, vec!["machine"]),
                ("no duplicate path", &same_file, 1, vec!["machine"]),
            ] {
                let ceilings: Vec<String> = files
                    .iter()
                    .map(|file| format!("set `defaults.invocation_lifetime_max` in {file}/settings.json"))
                    .collect();
                let edits: Vec<String> = ceilings
                    .iter()
                    .map(|edit| format!("{edit} above {allowance}"))
                    .collect();
                cases.push((
                    name,
                    b,
                    &lifetime,
                    Dimension::Invocations,
                    Contract::Lifetime { allowance },
                    allowance,
                    format!("{}, then {adjust}", edits.join(", ")),
                    None,
                    Remedy::RaiseThenAdjust { ceilings, allowance, command: command.clone() },
                ));
            }
        }
        let mut failures = Vec::new();
        for (name, bounds, journal, dimension, contract, bound, expected, instant, resolved) in cases {
            let error = refusal(dimension, contract, bound, window.day());
            let spent = error.exhaustion.as_deref().unwrap();
            let actual = budget_halt_remedy(spent, bounds, journal);
            assert_eq!(actual, resolved, "{name}: shared remedy choice");
            assert_eq!(actual.render(&bounds.halt_bound(spent)), expected, "{name}: rendering");
            assert_eq!(actual.renews_at(), instant.as_deref(), "{name}: renewal metadata");
            let text = budget_halt_text(&error, bounds, journal);
            let event = budget_halt_event(&error, bounds, journal, "plan.2", "work").unwrap();
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
    fn issue579_account_source_rendering() {
        let account = Bound::account(2);
        assert_eq!(account.source, BoundSource::Account);
        assert_eq!(BoundSource::Account.as_str(), "account");
        assert_eq!(account.effective, 2);
        assert_eq!(account.amount_phrase(None), "2 (account)");
        assert_eq!(account.requested, None);
        assert!(!account.delegated());
        assert!(!account.limited_by_ceiling());
        assert_eq!(account.ceiling_halt_phrase(None), None);
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
