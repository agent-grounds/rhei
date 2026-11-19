//! A spent program's actual exit route owns its completion debt, including
//! when a later run cannot spawn it. §FS-rhei-run-report.3.1 §FS-rhei-programs.3.2

use super::spent_visit_attention_support::section;
use super::*;

#[test]
fn issue749_spent_program_reports_actual_exit_route_on_both_runs() {
    let dir = unique_temp_dir("rhei-749-program-route");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Program route\n\n## Tasks\n\n### Task 1: Build\n**State:** build\n",
    );
    let program = write_python_agent(&dir, "build.py", "sys.exit(3)\n");
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: spent-program-route
version: 1
states:
  build:
    initial: true
    attempts: 1
    program:
      command: {}
    program_timeout: 20s
  inspect:
    gating: true
  failed:
    final: true
transitions:
  - {{ from: build, to: inspect, exit_code: 0 }}
  - {{ from: build, to: failed, exit_code: 3 }}
  - {{ from: inspect, to: failed }}
"#,
            fixture_command(&program)
        ),
    );
    let result = dir.join("runtime").join("results").join("plan.1.md");
    let owed = format!("result ({})", result.display());
    let mut failures = Vec::new();
    for run_number in 1..=2 {
        let output = rhei_command(dir.join("home"))
            .current_dir(&dir)
            .arg("--state-machine")
            .arg(&machine)
            .arg("run")
            .arg(&plan)
            .args(["--no-tui", "--no-dashboard", "--no-callbacks"])
            .output()
            .unwrap();
        let run = CliRun::from(&output);
        let context = format!("run {run_number}:\n{}\n{}", run.stdout, run.stderr);
        assert_eq!(run.status.code(), Some(1), "{context}");
        assert_eq!(run.stdout.contains("Spawning program"), run_number == 1, "{context}");
        assert_task_state(&plan, &machine, "1", "build");
        assert!(!result.exists(), "{context}");
        let records: Vec<serde_json::Value> = fs::read_dir(dir.join("runtime/spawns"))
            .unwrap()
            .map(|entry| serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap())
            .collect();
        assert_eq!(records.len(), 1, "{records:#?}\n{context}");
        let record = &records[0];
        for (key, expected) in [
            ("task", serde_json::json!("plan.1")),
            ("state", serde_json::json!("build")),
            ("kind", serde_json::json!("program")),
            ("ending", serde_json::json!("exited")),
            ("code", serde_json::json!(3)),
            ("moves", serde_json::json!(0)),
            ("attempt", serde_json::json!(1)),
            ("charged", serde_json::json!(1)),
            ("attempt_charged", serde_json::json!(true)),
        ] {
            assert_eq!(record[key], expected, "{key}: {record:#}\n{context}");
        }
        assert!(run.stderr.contains("1 attempts spent on this visit"), "{context}");
        if run_number == 1 {
            assert!(
                run.stderr.contains("program exited 3 but required outputs are missing"),
                "{context}"
            );
            assert!(run.stderr.contains(&owed), "actual completion debt:\n{context}");
        }
        let report = fs::read_to_string(dir.join("runtime/run-report.md")).unwrap();
        let mut surfaces = vec![("spent warning", run.stderr.as_str())];
        for title in ["Attention", "Transition Ledger", "Task Final States"] {
            let line = section(&report, title)
                .lines()
                .find(|line| {
                    line.contains("plan.1") && line.contains("attempts spent on this visit")
                })
                .unwrap_or_else(|| panic!("no current spent halt under {title}:\n{report}"));
            surfaces.push((title, line));
        }
        // Pipe capture need not emit the shared explanation; its generator is
        // covered separately. Check it too if this renderer emits it.
        if let Some(line) = run.stderr.lines().find(|line| line.contains("Task plan.1 (build):")) {
            surfaces.push(("shared halt explanation", line));
        }
        for (surface, line) in surfaces {
            if !line.contains(&owed) {
                failures.push(format!("run {run_number} {surface} lacks {owed}: {line}"));
            }
            if line.contains("names nothing on disk") {
                failures
                    .push(format!("run {run_number} {surface} denies the required result: {line}"));
            }
        }
        let attention = section(&report, "Attention");
        let retry = attention.find("rhei retry plan.1").expect("retry remedy");
        let rerun = attention.find("rerun").expect("rerun after retry");
        assert!(retry < rerun, "{attention}");
        assert!(!section(&report, "Waiting").contains("plan.1"), "{report}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
