// §AR-source-file-size.3

// Selecting and grouping what `rhei cost` totals: by run, by clock, and the
// group the records that name no run are reported in.

use std::path::Path;

use super::accounting_support::{
    accounting_workspace, last_run_id, seed_archived_records, ARCHIVED_RECORDS,
    ARCHIVED_TOKENS_2026_08_31, ARCHIVED_TOKENS_2026_09_01, ARCHIVED_TOKENS_2026_09_02,
    ARCHIVED_TOTAL_TOKENS, MOCK_AGENT_TOTAL_TOKENS, TERMINAL_PLAN, WORKING_PLAN,
};
use super::{
    assert_refuses_time_text, assert_stderr_contains, assert_success, run_cli, CliRun,
    HOSTILE_TIME_TEXTS,
};

/// The key the group of records that name no run is reported under.
/// §FS-rhei-cost-accounting.8.3
const UNATTRIBUTED_KEY: &str = "(unattributed)";

fn cost(plan: &Path, machine: &Path, args: &[&str]) -> CliRun {
    run_cli("cost", plan, machine, args)
}

fn cost_json(plan: &Path, machine: &Path, args: &[&str]) -> serde_json::Value {
    let mut with_json = args.to_vec();
    with_json.push("--json");
    let result = cost(plan, machine, &with_json);
    assert_success(&result);
    serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("cost --json should emit JSON ({err}); got:\n{}", result.stdout)
    })
}

fn total_tokens(summary: &serde_json::Value) -> u64 {
    summary["total"]["value"]
        .as_u64()
        .unwrap_or_else(|| panic!("summary should carry a measured total; got:\n{summary:#?}"))
}

fn group<'a>(payload: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    payload["groups"]
        .as_array()
        .expect("groups is an array")
        .iter()
        .find(|entry| entry["key"].as_str() == Some(key))
        .unwrap_or_else(|| panic!("no group keyed {key:?}; got:\n{:#?}", payload["groups"]))
}

/// A record that names no run is still counted, and the payload says how much
/// of the total came from records nothing could attribute — without asking for
/// a grouping, so the fact cannot be missed by a caller that never thought to.
// §FS-rhei-cost-accounting.8.4
#[test]
fn cost_json_names_the_unattributed_share_without_any_new_flag() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-unattributed", WORKING_PLAN);
    seed_archived_records(&dir);
    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));

    let payload = cost_json(&plan_path, &machine_path, &[]);
    let attribution = &payload["run_attribution"];
    assert_eq!(
        attribution["unattributed_invocation_count"].as_u64(),
        Some(ARCHIVED_RECORDS),
        "the six archived records name no run; got:\n{payload:#?}"
    );
    assert_eq!(
        attribution["attributed_invocation_count"].as_u64(),
        Some(1),
        "the run that just happened wrote exactly one attributed record"
    );
    assert_eq!(
        total_tokens(&attribution["unattributed"]),
        ARCHIVED_TOTAL_TOKENS,
        "the unattributed rollup is the archived records, not a rounding of them"
    );
    assert_eq!(
        payload["selection"]["invocation_count"].as_u64(),
        Some(ARCHIVED_RECORDS + 1),
        "no flag was given, so the selection is everything"
    );
}

/// Grouping by run must give the records that name no run a place of their own.
/// Dropping them, or folding them into whichever run happens to be there, loses
/// every record written before the field existed.
// §FS-rhei-cost-accounting.8.3
#[test]
fn cost_by_run_gives_the_unattributed_records_a_group_of_their_own() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-by-run", WORKING_PLAN);
    seed_archived_records(&dir);
    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    let run_id = last_run_id(&dir);

    let payload = cost_json(&plan_path, &machine_path, &["--by", "run"]);
    let unattributed = group(&payload, UNATTRIBUTED_KEY);
    assert_eq!(
        unattributed["unattributed"].as_bool(),
        Some(true),
        "the group is marked, so a machine reader need not match on its key"
    );
    assert_eq!(
        total_tokens(&unattributed["summary"]),
        ARCHIVED_TOTAL_TOKENS,
        "every archived record is in it"
    );
    assert_eq!(
        unattributed["summary"]["invocation_count"].as_u64(),
        Some(ARCHIVED_RECORDS),
        "all six of them, both agents"
    );

    let mine = group(&payload, &run_id);
    assert_eq!(
        total_tokens(&mine["summary"]),
        MOCK_AGENT_TOTAL_TOKENS,
        "the run's own group holds only what the run spent"
    );

    let text = cost(&plan_path, &machine_path, &["--by", "run"]);
    assert_success(&text);
    assert!(
        text.stdout.contains(UNATTRIBUTED_KEY),
        "the text listing names the group too; got:\n{}",
        text.stdout
    );
}

/// What the ticket asks for first: the cost of one invocation of `rhei run`.
/// The answer is never `complete` while records nothing could attribute are
/// sitting beside it, because one of them may belong to the run asked for.
// §FS-rhei-cost-accounting.8.2 §FS-rhei-cost-accounting.6.2
#[test]
fn cost_run_selects_one_runs_invocations_and_says_it_cannot_be_sure() {
    let (clean, clean_plan, clean_machine) = accounting_workspace("cost-run-clean", WORKING_PLAN);
    assert_success(&run_cli("run", &clean_plan, &clean_machine, &["--no-tui", "--no-callbacks"]));
    let clean_payload = cost_json(&clean_plan, &clean_machine, &["--run", &last_run_id(&clean)]);
    assert_eq!(
        clean_payload["summary"]["coverage"].as_str(),
        Some("complete"),
        "nothing is unattributed here, so the run's own total is the whole answer"
    );
    assert_eq!(total_tokens(&clean_payload["summary"]), MOCK_AGENT_TOTAL_TOKENS);

    let (dir, plan_path, machine_path) = accounting_workspace("cost-run-mixed", WORKING_PLAN);
    seed_archived_records(&dir);
    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));

    let payload = cost_json(&plan_path, &machine_path, &["--run", &last_run_id(&dir)]);
    assert_eq!(
        payload["selection"]["invocation_count"].as_u64(),
        Some(1),
        "only the run's own record is selected"
    );
    assert_eq!(
        total_tokens(&payload["summary"]),
        MOCK_AGENT_TOTAL_TOKENS,
        "and only what that record measured is totalled"
    );
    assert_eq!(
        payload["summary"]["coverage"].as_str(),
        Some("partial"),
        "six records could belong to this run and cannot say so; got:\n{payload:#?}"
    );
}

/// The reserved id, so the records nothing attributed can be asked for directly
/// rather than only found by grouping.
// §FS-rhei-cost-accounting.8.2
#[test]
fn cost_run_unattributed_selects_the_records_that_name_no_run() {
    let (dir, plan_path, machine_path) =
        accounting_workspace("cost-run-unattributed", TERMINAL_PLAN);
    seed_archived_records(&dir);

    let payload = cost_json(&plan_path, &machine_path, &["--run", "unattributed"]);
    assert_eq!(payload["selection"]["invocation_count"].as_u64(), Some(ARCHIVED_RECORDS));
    assert_eq!(total_tokens(&payload["summary"]), ARCHIVED_TOTAL_TOKENS);
}

/// A quota resets on a clock, so the clock has to be an axis. The archived
/// records fall on three UTC days, and each day is its own group.
// §FS-rhei-cost-accounting.8.3
#[test]
fn cost_by_day_groups_on_the_utc_calendar_day() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-by-day", TERMINAL_PLAN);
    seed_archived_records(&dir);

    let payload = cost_json(&plan_path, &machine_path, &["--by", "day"]);
    for (day, expected) in [
        ("2026-08-31", ARCHIVED_TOKENS_2026_08_31),
        ("2026-09-01", ARCHIVED_TOKENS_2026_09_01),
        ("2026-09-02", ARCHIVED_TOKENS_2026_09_02),
    ] {
        assert_eq!(
            total_tokens(&group(&payload, day)["summary"]),
            expected,
            "day {day} should total its own records; got:\n{payload:#?}"
        );
    }
}

/// The window, on the same records. `--until` is exclusive, so the two flags
/// name one day without the caller having to reason about the last second of it.
// §FS-rhei-cost-accounting.8.2
#[test]
fn cost_since_and_until_bound_the_window() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-window", TERMINAL_PLAN);
    seed_archived_records(&dir);

    let one_day =
        cost_json(&plan_path, &machine_path, &["--since", "2026-09-01", "--until", "2026-09-02"]);
    assert_eq!(total_tokens(&one_day["summary"]), ARCHIVED_TOKENS_2026_09_01);
    assert_eq!(one_day["selection"]["invocation_count"].as_u64(), Some(2));
    assert_eq!(one_day["selection"]["since"].as_str(), Some("2026-09-01T00:00:00Z"));
    assert_eq!(one_day["selection"]["until"].as_str(), Some("2026-09-02T00:00:00Z"));

    let from_the_last_day = cost_json(&plan_path, &machine_path, &["--since", "2026-09-02"]);
    assert_eq!(total_tokens(&from_the_last_day["summary"]), ARCHIVED_TOKENS_2026_09_02);

    let empty = cost(&plan_path, &machine_path, &["--since", "2026-09-03"]);
    assert_success(&empty);
    assert!(
        empty.stdout.contains("(no accounting records match the selection)"),
        "an empty window is a different answer from an empty workspace; got:\n{}",
        empty.stdout
    );
}

/// The promise to everyone already calling this: with none of the new flags,
/// nothing moves — save the one addition §FS-rhei-cost-accounting.8.4 now states, a line naming the
/// roots the empty answer searched, which a standalone workspace gets too.
// §FS-rhei-cost-accounting.8.4
#[test]
fn cost_with_no_new_flag_prints_what_it_printed_before() {
    let (empty_dir, empty_plan, empty_machine) =
        accounting_workspace("cost-compat-empty", TERMINAL_PLAN);
    let empty = cost(&empty_plan, &empty_machine, &[]);
    assert_success(&empty);
    assert_eq!(
        empty.stdout,
        format!(
            "(no accounting records found)\nsearched {}\n",
            empty_dir.join("runtime/accounting").display()
        ),
        "the first line is byte for byte what it was, and one root is named inline"
    );
    drop(empty_dir);

    let (dir, plan_path, machine_path) = accounting_workspace("cost-compat", TERMINAL_PLAN);
    seed_archived_records(&dir);
    let seeded = cost(&plan_path, &machine_path, &[]);
    assert_success(&seeded);
    assert!(
        seeded.stdout.starts_with(
            "Cost unpriced | Total 34.4M | In 34.3M | Out 157.7k | Coverage Partial | Invocations 6\n"
        ),
        "the unselected reading is unchanged; got:\n{}",
        seeded.stdout
    );
    assert!(seeded.stdout.contains("\nBy Node:\n"), "got:\n{}", seeded.stdout);
    assert!(seeded.stdout.contains("\nHighest subtree nodes:\n"), "got:\n{}", seeded.stdout);
}

/// A `<TIME>` too large to hold is a usage error like any other unreadable one.
/// Multiplying it out unguarded panics a debug build and wraps a release one
/// into a window the caller never asked for — the silently wrong answer this
/// flag exists to refuse.
// §FS-rhei-cost-accounting.8.2
#[test]
fn cost_refuses_a_duration_too_large_to_hold_rather_than_wrapping_it() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-overflow", TERMINAL_PLAN);
    seed_archived_records(&dir);

    for text in ["18446744073709551615d", "999999999999999999h"] {
        let refused = cost(&plan_path, &machine_path, &["--since", text]);
        assert!(
            !refused.status.success(),
            "--since {text} must be refused, not answered; got:\n{}",
            refused.stdout
        );
        assert_stderr_contains(&refused, &format!("could not read '{text}' as a time for --since"));
    }

    // The same guard on the other end of the window.
    let refused = cost(&plan_path, &machine_path, &["--until", "18446744073709551615d"]);
    assert!(!refused.status.success(), "got:\n{}", refused.stdout);
    assert_stderr_contains(&refused, "as a time for --until");
}

/// A `<TIME>` is arbitrary text, and the parser is the first thing it meets.
/// Reading it by byte index crashes on any value whose last character is
/// multi-byte — a pasted trailing non-breaking space is the ordinary way to get
/// one — and a raw panic is neither the refusal §FS-rhei-cost-accounting.8.2 requires nor a message
/// anyone can act on. Both ends of the window are held to it.
// §FS-rhei-cost-accounting.8.2
#[test]
fn cost_refuses_time_text_it_cannot_read_rather_than_crashing_on_it() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-hostile-time", TERMINAL_PLAN);
    seed_archived_records(&dir);

    for text in HOSTILE_TIME_TEXTS {
        for flag in ["--since", "--until"] {
            let refused = cost(&plan_path, &machine_path, &[flag, text]);
            assert_refuses_time_text(&refused, flag, text);
        }
    }
}
