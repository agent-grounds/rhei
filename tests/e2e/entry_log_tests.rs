//! Every entry into a state keeps its own log, report and metric link, even
//! where the state keeps no visit counter, and no spawn ever truncates a log
//! it cannot account for (agent-grounds/rhei#309).

// §FS-rhei-agents.8.1 §FS-rhei-programs.5.1 §FS-rhei-session-reports.1 §FS-rhei-metrics.3

use std::fs;

use super::entry_log_support::*;
use super::*;

/// The loop the ticket was found in: `measure ⇄ cover`, `cover` with no
/// `visits:`. Entry 1 keeps the plain name, entry 2 is `-2`, each has its own
/// report, and each metric iteration links its own session.
// §FS-rhei-agents.8.1 §FS-rhei-session-reports.1 §FS-rhei-metrics.3 §FS-rhei-metrics.4
#[test]
fn an_uncounted_loop_keeps_one_log_and_one_report_per_entry() {
    let (dir, plan, machine) = loop_fixture("entry-log-loop", COVER);

    assert_success(&run_cli("run", &plan, &machine, &RUN));
    assert_task_state(&plan, &machine, "1", "done");

    let first = read(&dir, "runtime/logs/task-plan.1-cover.log");
    assert!(
        first.contains("SESSION-1-content") && !first.contains("SESSION-2-content"),
        "entry 1's transcript must survive entry 2; task-plan.1-cover.log holds:\n{first}"
    );
    let second = read(&dir, "runtime/logs/task-plan.1-cover-2.log");
    assert!(second.contains("SESSION-2-content"), "entry 2 writes `-2`:\n{second}");
    assert_eq!(
        names_in(&dir, "runtime/logs"),
        [
            "task-plan.1-cover-2.log",
            "task-plan.1-cover.log",
            "task-plan.1-measure-2.log",
            "task-plan.1-measure-3.log",
            "task-plan.1-measure.log",
        ],
        "one log per entry, programs included (§FS-rhei-programs.5.1)"
    );

    assert!(read(&dir, "runtime/reports/task-plan.1-cover.md").contains("SESSION-1-content"));
    assert!(read(&dir, "runtime/reports/task-plan.1-cover-2.md").contains("SESSION-2-content"));

    let records: Vec<serde_json::Value> = read(&dir, "runtime/metrics/coverage.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("metric record"))
        .collect();
    let cover = |iteration: u64| -> (String, u64) {
        let record = records
            .iter()
            .find(|record| record["iteration"] == iteration)
            .unwrap_or_else(|| panic!("iteration {iteration} recorded: {records:?}"));
        let session = record["sessions"]
            .as_array()
            .and_then(|sessions| sessions.iter().find(|session| session["state"] == "cover"))
            .unwrap_or_else(|| panic!("iteration {iteration} binds a cover session: {record}"));
        (
            session["log"].as_str().expect("log").to_string(),
            session["visit"].as_u64().expect("visit"),
        )
    };
    assert_eq!(
        [cover(1), cover(2)],
        [
            ("runtime/logs/task-plan.1-cover.log".to_string(), 1),
            ("runtime/logs/task-plan.1-cover-2.log".to_string(), 2),
        ],
        "two metric iterations link two sessions, shown as cover #1 and cover #2"
    );
}

/// A failed attempt inside entry 2 is retried as `-2-attempt2`, and the retry's
/// prompt still says it is a retry and names entry 2's first transcript.
// §FS-rhei-agents.8.1 §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
#[test]
fn a_retry_inside_a_later_entry_is_numbered_after_that_entry() {
    let fails_once = format!("{COVER}if n == 2:\n    raise SystemExit(1)\n");
    let (dir, plan, machine) = loop_fixture("entry-log-retry", &fails_once);

    // The failed attempt stops the first run; the next run retries the same entry.
    let failed = run_cli("run", &plan, &machine, &RUN);
    assert!(!failed.status.success(), "entry 2's first attempt exits 1");
    assert_task_state(&plan, &machine, "1", "cover");
    assert_success(&run_cli("run", &plan, &machine, &RUN));
    assert_task_state(&plan, &machine, "1", "done");

    let retry = read(&dir, "runtime/logs/task-plan.1-cover-2-attempt2.log");
    assert!(retry.contains("SESSION-3-content"), "the retry of entry 2:\n{retry}");
    assert!(read(&dir, "runtime/logs/task-plan.1-cover-2.log").contains("SESSION-2-content"));
    assert!(read(&dir, "runtime/logs/task-plan.1-cover.log").contains("SESSION-1-content"));

    let prompt = read(&dir, "runtime/prompts/cover-3.md");
    assert!(
        prompt.contains("Retrying this visit: attempt 2.")
            && prompt.contains("task-plan.1-cover-2.log`"),
        "the retry paragraph finds entry 2's record:\n{prompt}"
    );
    assert!(
        !read(&dir, "runtime/prompts/cover-2.md").contains("Previous log:"),
        "an uncounted re-entry still renders no `Previous log:` line"
    );
}

const COUNTED_MEASURE_MACHINE: &str = r#"name: counted-program
version: 1
states:
  measure:
    initial: true
    description: Measure, twice
    visits: 2
    program:
      command: COMMAND
    program_timeout: 20s
  cover:
    description: Raise coverage
    agent: mock
    agent_timeout: 20s
  done:
    description: Done
    final: true
transitions:
  - { from: measure, to: done, condition: visitCount >= visits, description: Enough }
  - { from: measure, to: cover, condition: visitCount < visits, description: Again }
  - { from: cover, to: measure, description: Measure again }
"#;

/// A program state with `visits:` gets the documented `-{visit_count}` name on
/// its second visit instead of overwriting the first.
// §FS-rhei-programs.5.1
#[test]
fn a_counted_program_writes_its_second_visit_beside_the_first() {
    let dir = unique_temp_dir("entry-log-counted-program");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TICKET);
    let measure = write_python_agent(
        &dir,
        "measure.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'runtime' / 'measure-spawns.txt'
n = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(n))
result('measured\n')
sys.stdout.write('MEASURE-VISIT-{}\n'.format(n))
"#,
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &COUNTED_MEASURE_MACHINE.replace("COMMAND", &fixture_command(&measure)),
    );
    let agent = write_python_agent(&dir, "cover.py", COVER);
    settings(&dir, &agent);

    assert_success(&run_cli("run", &plan, &machine, &RUN));
    assert_task_state(&plan, &machine, "1", "done");

    let first = read(&dir, "runtime/logs/task-plan.1-measure.log");
    assert!(
        first.contains("MEASURE-VISIT-1") && !first.contains("MEASURE-VISIT-2"),
        "visit 1's program log must survive visit 2; it holds:\n{first}"
    );
    assert!(read(&dir, "runtime/logs/task-plan.1-measure-2.log").contains("MEASURE-VISIT-2"));
}

/// A file already at the name the next entry would use, with no spawn record
/// behind it, is refused rather than overwritten, the worker does not run, and
/// the refusal says what to do about it.
// §FS-rhei-agents.8.1
#[test]
fn a_spawn_refuses_a_log_path_no_record_accounts_for() {
    let (dir, plan, machine) = gated_fixture("entry-log-refuse");
    assert_success(&run_cli("run", &plan, &machine, &RUN));
    let planted = dir.join("runtime/logs/task-plan.1-work-2.log");
    fs::write(&planted, "PLANTED, not rhei's\n").expect("plant a log");
    assert_success(&run_transition(&plan, &machine, "1", "verifying", "work"));

    let run = run_cli("run", &plan, &machine, &RUN);
    let combined = format!("{}{}", run.stdout, run.stderr);

    assert!(
        combined.contains("refusing to spawn: ")
            && combined
                .contains("task-plan.1-work-2.log exists and no spawn record accounts for it"),
        "an unaccounted log path is refused by name; logs={:?}\n{combined}",
        names_in(&dir, "runtime/logs")
    );
    assert!(
        combined.contains("move that file away, or run `rhei reset --rhei plan`"),
        "the refusal carries its remedy, naming the ticket's rhei:\n{combined}"
    );
    assert_eq!(fs::read_to_string(&planted).expect("planted log"), "PLANTED, not rhei's\n");
    let first = read(&dir, "runtime/logs/task-plan.1-work.log");
    assert!(first.contains("WORK-plan.1-1"), "entry 1's log is not overwritten either:\n{first}");
    assert_eq!(read(&dir, "runtime/work-spawns-plan.1.txt"), "1", "no worker ran");
    assert_task_state(&plan, &machine, "1", "work");
}

/// A narrowed reset sweeps the ticket's reports with its logs, so its next
/// entry starts again at the plain name, and a sibling rhei keeps its files.
// §FS-rhei-reset.2.1 §FS-rhei-agents.8.3 §FS-rhei-transitions.4.3
#[test]
fn a_narrowed_reset_restarts_entry_numbering_and_sweeps_the_tickets_reports() {
    let dir = unique_temp_dir("entry-log-reset");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("project");
    fs::write(project.join("index.panta.md"), "# Panta: Entry logs\n").expect("index");
    let ticket = ONE_TICKET.replace("**State:** measure", "**State:** work");
    for name in ["alpha", "beta"] {
        fs::write(project.join(format!("{name}.rhei.md")), &ticket).expect("rhei");
    }
    let machine = write_fixture_file(&dir, "states.yaml", GATED_MACHINE);
    let agent = write_python_agent(&dir, "work.py", WORK);
    settings(&project, &agent);

    assert_success(&run_cli("run", &project, &machine, &RUN));
    for _ in 0..2 {
        assert_success(&run_transition(&project, &machine, "alpha.1", "verifying", "work"));
        assert_success(&run_cli("run", &project, &machine, &RUN));
    }
    let alpha_files = |sub: &str| -> Vec<String> {
        names_in(&project, sub)
            .into_iter()
            .filter(|name| name.starts_with("task-alpha.1-"))
            .collect()
    };
    assert!(!alpha_files("runtime/reports").is_empty(), "alpha ran three entries");
    let beta_log = read(&project, "runtime/logs/task-beta.1-work.log");
    let beta_report = read(&project, "runtime/reports/task-beta.1-work.md");

    assert_success(&run_cli("reset", &project, &machine, &["--rhei", "alpha", "--yes"]));
    assert_eq!(
        alpha_files("runtime/reports"),
        Vec::<String>::new(),
        "a narrowed reset removes runtime/reports/task-<ticket-id>-*"
    );
    assert_eq!(alpha_files("runtime/logs"), Vec::<String>::new());
    assert_eq!(read(&project, "runtime/logs/task-beta.1-work.log"), beta_log);
    assert_eq!(read(&project, "runtime/reports/task-beta.1-work.md"), beta_report);

    assert_success(&run_cli("run", &project, &machine, &RUN));
    assert_eq!(
        alpha_files("runtime/logs"),
        ["task-alpha.1-work.log"],
        "the entry after a reset is entry 1 again"
    );
}

/// A report already at the stem a spawn has just created its log under cannot
/// belong to that log: it is renamed aside, said once, and the new report is
/// written in its place.
// §FS-rhei-session-reports.1
#[test]
fn an_orphan_report_at_a_new_stem_is_renamed_aside() {
    let (dir, plan, machine) = gated_fixture("entry-log-orphan");
    assert_success(&run_cli("run", &plan, &machine, &RUN));
    let orphan = dir.join("runtime/reports/task-plan.1-work-2.md");
    fs::write(&orphan, "ORPHAN report, no log\n").expect("plant a report");
    assert_success(&run_transition(&plan, &machine, "1", "verifying", "work"));

    let run = run_cli("run", &plan, &machine, &RUN);
    let combined = format!("{}{}", run.stdout, run.stderr);
    assert_success(&run);

    let renamed: Vec<String> = names_in(&dir, "runtime/reports")
        .into_iter()
        .filter(|name| {
            name.strip_prefix("task-plan.1-work-2.orphaned-")
                .and_then(|rest| rest.strip_suffix(".md"))
                .is_some_and(|ts| !ts.is_empty() && ts.chars().all(|c| c.is_ascii_digit()))
        })
        .collect();
    assert_eq!(
        renamed.len(),
        1,
        "the orphan is renamed to <stem>.orphaned-<unix-ts>.md; reports={:?}\n{combined}",
        names_in(&dir, "runtime/reports")
    );
    assert_eq!(read(&dir, &format!("runtime/reports/{}", renamed[0])), "ORPHAN report, no log\n");
    assert!(read(&dir, "runtime/reports/task-plan.1-work-2.md").contains("WORK-plan.1-2"));
    assert!(
        combined
            .lines()
            .any(|line| line.contains("task-plan.1-work-2.md") && line.contains(&renamed[0])),
        "one line names both paths:\n{combined}"
    );
}
