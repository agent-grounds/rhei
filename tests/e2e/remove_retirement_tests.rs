//! `rhei remove`: what cleanup may touch, and what a retired id forbids
//! afterwards — in every layout that holds a retirement record.
//! §FS-rhei-remove.4.2 §FS-rhei-remove.5

use std::fs;

use super::budget_support::{
    run_at, run_plan, setup_with_agent, FINISHING_AGENT, FINISHING_MACHINE,
};
use super::into_support::{host_single_file, run_into, write_review_template};
use super::new_tests::{assert_failure, flattened_output, new_run, project_with_rhei};
use super::*;

/// The header rhei writes before it starts a process: nothing ran.
fn pre_spawn_log(dir: &Path, id: &str) -> PathBuf {
    let path = dir.join(format!("runtime/logs/task-{id}-pending.log"));
    fs::create_dir_all(path.parent().expect("logs dir")).expect("logs dir");
    fs::write(&path, format!("=== rhei agent log v1 ===\nstate: pending\ntask: {id}\n===\n"))
        .expect("pre-spawn log");
    path
}

/// §FS-rhei-remove.4.2: ownership is exact — removing `auth.1` cleans its own
/// export home and pre-spawn log and never reaches `auth.10`'s.
#[test]
fn cleanup_never_reaches_a_sibling_whose_id_extends_it() {
    let dir = project_with_rhei("remove-sibling-safe");
    for n in 1..=10 {
        assert_success(&new_run(&["new", &format!("T{n}"), "--under", "auth"], &dir));
    }
    for id in ["auth.1", "auth.10"] {
        fs::create_dir_all(dir.join(format!("runtime/exports/{id}"))).expect("export home");
        pre_spawn_log(&dir, id);
    }

    let result = new_run(&["remove", "auth.1"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("deleted runtime/exports/auth.1"), "got:\n{}", result.stdout);
    assert!(result.stdout.contains("deleted runtime/logs/task-auth.1-pending.log"));
    assert!(!dir.join("runtime/exports/auth.1").exists());
    assert!(dir.join("runtime/exports/auth.10").is_dir(), "auth.10's export home stays");
    assert!(dir.join("runtime/logs/task-auth.10-pending.log").is_file(), "auth.10's log stays");
}

/// §FS-rhei-remove.4.2: a named id's log prefix also prefixes a sibling's
/// (`auth.fix-` and `auth.fix-cache-…`), so that file is ambiguous and is
/// refused, not cleaned.
#[test]
fn an_ambiguous_named_id_path_is_refused_not_cleaned() {
    let dir = project_with_rhei("remove-named-ambiguous");
    assert_success(&new_run(&["new", "Fix", "--under", "auth", "--id", "fix"], &dir));
    assert_success(&new_run(&["new", "Cache", "--under", "auth", "--id", "fix-cache"], &dir));
    let theirs = pre_spawn_log(&dir, "auth.fix-cache");

    let result = new_run(&["remove", "auth.fix"], &dir);
    assert_failure(&result, "auth.fix cannot be removed");
    assert!(flattened_output(&result).contains("may belong to another ticket"));
    assert!(theirs.is_file(), "the sibling's file is never touched");
    assert!(fs::read_to_string(dir.join("auth.rhei.md")).expect("plan").contains("Task fix:"));
}

/// §FS-rhei-remove.4.2: a symlink is refused, not followed or cleaned.
#[cfg(unix)]
#[test]
fn a_symlinked_residue_path_is_refused() {
    let dir = project_with_rhei("remove-symlink");
    assert_success(&new_run(&["new", "Mistake", "--under", "auth"], &dir));
    let outside = unique_temp_dir("remove-symlink-outside");
    fs::create_dir_all(dir.join("runtime/exports")).expect("exports");
    std::os::unix::fs::symlink(&*outside, dir.join("runtime/exports/auth.1")).expect("symlink");

    let result = new_run(&["remove", "auth.1"], &dir);
    assert_failure(&result, "auth.1 cannot be removed");
    let said = flattened_output(&result);
    assert!(said.contains("runtime/exports/auth.1 is a symlink"), "got:\n{said}");
    assert!(!said.contains("it has history"), "an unsafe path is not history; got:\n{said}");
    assert!(outside.is_dir(), "the target is never reached");
    assert!(fs::symlink_metadata(dir.join("runtime/exports/auth.1")).is_ok(), "the link stays");
}

/// §FS-rhei-remove.5.2, §FS-rhei-validate.4: a hand-written definition under a
/// retired id does not validate, and neither does a `**Prior:**` naming it —
/// which is diagnosed as retired, not unknown.
#[test]
fn a_retired_id_cannot_come_back_by_hand_or_be_depended_on() {
    let dir = project_with_rhei("remove-hand-resurrect");
    assert_success(&new_run(&["new", "Keep", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Mistake", "--under", "auth"], &dir));
    assert_success(&new_run(&["remove", "auth.2"], &dir));
    let plan = dir.join("auth.rhei.md");
    let clean = fs::read_to_string(&plan).expect("plan");

    fs::write(&plan, format!("{clean}\n### Task 2: Back again\n**State:** pending\n"))
        .expect("edit");
    let result = new_run(&["validate"], &dir);
    assert!(!result.status.success(), "a resurrected id must not validate");
    let said = flattened_output(&result);
    assert!(said.contains("auth.2 was removed and its id is retired"), "got:\n{said}");

    fs::write(
        &plan,
        format!("{clean}\n### Task 3: Later\n**State:** pending\n**Prior:** auth.2\n"),
    )
    .expect("edit");
    let result = new_run(&["validate"], &dir);
    assert!(!result.status.success(), "a retired Prior must not validate");
    let said = flattened_output(&result);
    assert!(said.contains("depends on retired Task auth.2"), "got:\n{said}");
}

/// §FS-rhei-remove.5.1, §FS-rhei-validate.4: the key is reserved — a value that
/// is not a retirement map refuses removal and validation, and is never
/// overwritten.
#[test]
fn a_foreign_retired_tickets_value_is_never_overwritten() {
    let dir = project_with_rhei("remove-reserved-key");
    assert_success(&new_run(&["new", "Mistake", "--under", "auth"], &dir));
    let manifest = dir.join("index.panta.md");
    let foreign = "# Panta: Test\n---\nmetadata:\n  retiredTickets: mine\n---\n";
    fs::write(&manifest, foreign).expect("manifest");

    assert_failure(&new_run(&["remove", "auth.1"], &dir), "metadata.retiredTickets");
    assert_eq!(fs::read_to_string(&manifest).expect("manifest"), foreign);
    assert_failure(&new_run(&["validate"], &dir), "retiredTickets");
}

/// §FS-rhei-remove.5.1: a lone single-file plan with no project manifest holds
/// its own retirement record in its frontmatter.
#[test]
fn a_lone_plan_records_its_own_retirement() {
    let dir = unique_temp_dir("remove-lone-plan");
    let plan = write_fixture_file(
        &dir,
        "solo.rhei.md",
        "# Rhei: Solo\n\n## Tasks\n\n### Task 1: Keep\n**State:** pending\n\n### Task 2: Mistake\n**State:** pending\n",
    );
    let result = new_run(&["remove", "solo.2"], &dir);
    assert_success(&result);
    let text = fs::read_to_string(&plan).expect("plan");
    assert!(text.contains("retiredTickets") && text.contains("solo.2"), "got:\n{text}");
    assert!(!text.contains("Mistake") && text.contains("### Task 1: Keep"), "got:\n{text}");
    assert!(!dir.join("index.panta.md").exists(), "no manifest is invented");
    assert_success(&new_run(&["validate"], &dir));
}

/// §FS-rhei-library.4: placement refuses to put a template ticket under a
/// retired id, and writes nothing.
#[test]
fn template_placement_refuses_a_retired_id() {
    let (dir, _root) = host_single_file("remove-placement");
    write_review_template(&dir);
    let plan = dir.join("release.rhei.md");
    let host = fs::read_to_string(&plan).expect("plan");
    fs::write(&plan, format!("{host}\n### Task coordinate: Mistake\n**State:** pending\n"))
        .expect("edit");
    assert_success(&run_into(&["remove", "release.coordinate"], &dir));
    let before = fs::read_to_string(&plan).expect("plan");

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_failure(&result, "release.coordinate is retired");
    assert_eq!(fs::read_to_string(&plan).expect("plan"), before, "placement wrote nothing");
}

/// §FS-rhei-remove.5.1, §FS-rhei-budgets.5.2: removal keeps the ticket's budget
/// identity in its retirement and never rebinds it to another ticket.
#[test]
fn a_retired_budget_identity_is_never_rebound() {
    const UUID: &str = "7d0c3f1e-0000-4000-8000-000000000002";
    let dir = unique_temp_dir("remove-budget-binding");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &format!(
            "# Rhei: Plan\n---\nmetadata:\n  tasks:\n    \"2\":\n      budgetTicketId: {UUID}\n---\n\n\
             ## Tasks\n\n### Task 1: Keep\n**State:** pending\n\n### Task 2: Mistake\n**State:** pending\n"
        ),
    );
    assert_success(&new_run(&["remove", "plan.rhei.md", "--task", "2"], &dir));
    let text = fs::read_to_string(&plan).expect("plan");
    assert!(text.contains(&format!("budgetTicketId: {UUID}")), "kept in the retirement:\n{text}");

    let rebound = text.replacen(
        "  retiredTickets:",
        &format!("  tasks:\n    '3':\n      budgetTicketId: {UUID}\n  retiredTickets:"),
        1,
    );
    fs::write(&plan, format!("{rebound}\n### Task 3: New\n**State:** pending\n")).expect("edit");
    let result = new_run(&["validate"], &dir);
    assert!(!result.status.success(), "a rebound identity must not validate");
    assert!(flattened_output(&result).contains("belongs to retired Task plan.2"));
}

/// Every budget journal and witness file under `dir`, with its bytes.
fn budget_files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if (path.extension().is_some_and(|ext| ext == "jsonl")
                && path.to_string_lossy().contains("budget"))
                || path.file_name().is_some_and(|name| name == "journal.jsonl")
            {
                out.push((path.clone(), fs::read(&path).expect("readable")));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

/// §FS-rhei-remove.4.2: removal deletes no budget data — the account a real
/// run wrote is byte-identical after an untouched sibling is removed.
#[test]
fn removal_leaves_the_budget_journal_byte_identical() {
    let (dir, plan, machine) =
        setup_with_agent("remove-budget-journal", FINISHING_MACHINE, FINISHING_AGENT, "");
    assert_success(&run_plan(&plan, &machine, None));
    let ran = fs::read_to_string(&plan).expect("plan");
    fs::write(&plan, format!("{ran}\n### Task 2: Mistake\n**State:** work\n")).expect("edit");
    let before = budget_files(&dir);
    assert!(!before.is_empty(), "the run should have written a budget journal");

    let result = run_at("remove", &plan, &machine, None, &["--task", "2"]);
    assert_success(&result);
    assert_eq!(budget_files(&dir), before, "removal must not touch budget data");
}
