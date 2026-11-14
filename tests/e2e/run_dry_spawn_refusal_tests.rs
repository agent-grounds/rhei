//! Dry-run predictions must use the real spawn planner for pending work.
//! §FS-rhei-run.4 §FS-rhei-agents.8.1

use super::roster_support::tree_snapshot;
use super::*;

struct RefusalFixture {
    dir: TestDir,
    plan: PathBuf,
    machine: PathBuf,
    log: PathBuf,
}

enum Refusal {
    UnaccountedLog,
    UnreadableLedger,
}

impl RefusalFixture {
    fn new(kind: &str) -> Self {
        let dir = unique_temp_dir(&format!("rhei-662-dry-refusal-{kind}"));
        let worker = write_python_agent(
            &dir,
            "worker.py",
            "write(pathlib.Path(__file__).parent / 'executed', 'worker ran\\n')\n\
             result('Work completed.\\n')\n",
        );
        let settings = dir.join(".agent-grounds/rhei");
        fs::create_dir_all(&settings).expect("settings directory");
        fs::write(
            settings.join("settings.json"),
            format!(
                r#"{{"defaults": {{"agent": "fake", "agent_timeout": "60s"}},
                "agents": {{"fake": {{"command": {}, "stdin_prompt": true,
                                      "intervene_stdin": false}}}}}}"#,
                fixture_command(&worker)
            ),
        )
        .expect("local fake agent settings");
        let work = match kind {
            "agent" => "agent: fake, agent_timeout: 60s".to_string(),
            "program" => {
                format!("program: {{command: {}}}, program_timeout: 60s", fixture_command(&worker))
            }
            _ => panic!("unknown work kind {kind}"),
        };
        let callback = python_callback_yaml(&format!(
            "from pathlib import Path; Path({:?}).write_text('callback ran')",
            dir.join("callback-executed").to_string_lossy()
        ));
        let machine = write_fixture_file(
            &dir,
            "states.yaml",
            &format!(
                r#"name: dry-run-vs-run
version: 1
states:
  work: {{initial: true, {work}}}
  done: {{final: true}}
transitions:
  - {{from: work, to: done, callback: {callback}}}
"#
            ),
        );
        let plan = write_fixture_file(
            &dir,
            "plan.rhei.md",
            "# Rhei: Dry run before a resume\n\n## Tasks\n\n\
             ### Task 1: Cover one library\n**State:** work\n",
        );
        fs::create_dir_all(dir.join("runtime/logs")).expect("logs directory");
        // First entry, first attempt, no fan-out: no identity or number suffix.
        let log = dir.join("runtime").join("logs").join("task-plan.1-work.log");
        fs::write(&log, b"partial output of an earlier visit\n").expect("unaccounted log");
        assert!(!dir.join("runtime/spawns").exists(), "no accounting record");
        // Prove admission succeeds, and initialize the shared CLI root guard
        // in the isolated home before snapshotting every path and file byte.
        assert_success(&run_cli("validate", &plan, &machine, &[]));
        Self { dir, plan, machine, log }
    }

    fn run(&self, parallel: &str, dry: bool, json: bool) -> CliRun {
        let mut args = vec!["--no-tui", "--parallel", parallel];
        if dry {
            args.push("--dry-run");
        }
        if json {
            args.push("--json");
        }
        run_cli("run", &self.plan, &self.machine, &args)
    }
}

fn said(out: &CliRun) -> String {
    format!("{}{}", out.stdout, out.stderr)
}

fn json_strings(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(values) => {
            values.iter().map(json_strings).collect::<Vec<_>>().join("\n")
        }
        serde_json::Value::Object(fields) => {
            fields.values().map(json_strings).collect::<Vec<_>>().join("\n")
        }
        _ => String::new(),
    }
}

/// Compare on one fixture, snapshotting dry run before the real run can write
/// its descriptor/report. The real refusal is asserted before the prediction,
/// so a setup/admission/executable failure cannot masquerade as this defect.
fn refusal_prediction(kind: &str, parallel: &str, cause: Refusal) {
    let fixture = RefusalFixture::new(kind);
    let mut expected = vec!["refusing to spawn:".to_string()];
    match cause {
        Refusal::UnaccountedLog => expected.extend([
            fixture.log.to_str().unwrap().to_string(),
            "exists and no spawn record accounts for it".to_string(),
            "move that file away".to_string(),
            "rhei reset".to_string(),
            "--rhei plan".to_string(),
        ]),
        Refusal::UnreadableLedger => {
            fs::remove_file(&fixture.log).expect("no log conflict in the ledger case");
            let ledger = fixture.dir.join("runtime").join("state-transitions.log");
            // A directory is unreadable as a ledger on every platform, even
            // when tests run with privileges that bypass file permissions.
            fs::create_dir(&ledger).expect("unreadable ledger");
            expected.extend([
                ledger.to_str().unwrap().to_string(),
                "cannot be read".to_string(),
                "entry number that names this log is unknown".to_string(),
            ]);
        }
    }
    let before = tree_snapshot(&fixture.dir);
    let plan_before = fs::read(&fixture.plan).expect("plan bytes");
    let log_before = fs::read(&fixture.log).ok();
    let mut previews = Vec::new();
    for json in [false, true] {
        let dry = fixture.run(parallel, true, json);
        assert_eq!(
            tree_snapshot(&fixture.dir),
            before,
            "dry run changes no path or byte, including markdown, log, record, lock, home or sentinel"
        );
        if json {
            assert!(!dry.stdout.trim().is_empty(), "JSON run emits records");
            for line in dry.stdout.lines() {
                serde_json::from_str::<serde_json::Value>(line)
                    .unwrap_or_else(|err| panic!("stdout must remain pure JSONL: {err}: {line}"));
            }
        }
        previews.push((json, dry));
    }
    let real = fixture.run(parallel, false, false);
    let real_text = said(&real);
    assert_eq!(real.status.code(), Some(1), "real spawn is refused:\n{real_text}");
    for fragment in &expected {
        assert!(real_text.contains(fragment), "real refusal names {fragment}:\n{real_text}");
    }
    assert_eq!(fs::read(&fixture.plan).unwrap(), plan_before, "real refusal preserves task state");
    assert_eq!(fs::read(&fixture.log).ok(), log_before, "real refusal preserves log allocation");
    assert!(!fixture.dir.join("executed").exists(), "no agent or program runs");
    assert!(!fixture.dir.join("callback-executed").exists(), "no callback runs");

    // Aggregate both frontend failures so the red output retains both predictions.
    let mut failures = Vec::new();
    for (json, dry) in previews {
        let text = said(&dry);
        let diagnostic = if json {
            // Decode JSON strings before checking paths (Windows escapes '\\').
            dry.stdout
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                .map(|value| json_strings(&value))
                .collect::<Vec<_>>()
                .join("\n")
                + &dry.stderr
        } else {
            text.clone()
        };
        let refused = dry.status.code() == real.status.code()
            && expected
                .iter()
                // The frontend may prefix the planner's 'refusing to spawn'
                // with 'would'; the concrete cause/remedy remains identical.
                .skip(1)
                .all(|fragment| diagnostic.contains(fragment));
        if !refused
            || diagnostic.contains("Would spawn:")
            || diagnostic.contains("would transition:")
        {
            failures.push(format!(
                "json={json}: expected refusal/status 1 with path and remedy, no success projection; \
                 dry exit={:?}\n{dry_text}",
                dry.status.code(),
                dry_text = said(&dry)
            ));
        }
    }
    assert!(failures.is_empty(), "{kind}, parallel={parallel}:\n{}", failures.join("\n"));
}

#[test]
fn dry_run_refuses_unaccounted_agent_log_sequential() {
    refusal_prediction("agent", "1", Refusal::UnaccountedLog);
}

#[test]
fn dry_run_refuses_unaccounted_agent_log_pooled() {
    refusal_prediction("agent", "2", Refusal::UnaccountedLog);
}

#[test]
fn dry_run_refuses_unaccounted_program_log_sequential() {
    refusal_prediction("program", "1", Refusal::UnaccountedLog);
}

#[test]
fn dry_run_refuses_unaccounted_program_log_pooled() {
    refusal_prediction("program", "2", Refusal::UnaccountedLog);
}

#[test]
fn dry_run_refuses_unreadable_ledger_for_agent() {
    refusal_prediction("agent", "1", Refusal::UnreadableLedger);
}

#[test]
fn dry_run_refuses_unreadable_ledger_for_pooled_program() {
    refusal_prediction("program", "2", Refusal::UnreadableLedger);
}
