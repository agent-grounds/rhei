//! Implementation-owned boundary coverage for export-prior migration.

use std::fs;
use std::path::{Path, PathBuf};

use super::terminal_result_tests::write_mock_agent_settings;
use super::*;

const MACHINE: &str = r#"name: migration-boundaries
version: 1
states:
  pending:
    initial: true
    description: Work
    agent: mock
    agent_timeout: 10s
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
"#;

const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
write(root / 'runtime' / ('spawned-' + task + '.txt'), agent_prompt())
result('done\n')
"#;

fn setup(prefix: &str, producer_state: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &format!(
            "# Rhei: boundaries\n**States:** states\n\n## Tasks\n\n\
             ### Task 1: producer\n**State:** {producer_state}\n**Provides:** x\n\n\
             ### Task 2: consumer\n**State:** pending\n**Consumes:** 1:x\n"
        ),
    );
    write_fixture_file(&dir, "states.yaml", MACHINE);
    (dir, plan)
}

fn migrate(home: &Path, target: Option<&Path>, dry_run: bool) -> CliRun {
    let mut command = rhei_command(home);
    command.arg("migrate").arg("export-priors");
    if dry_run {
        command.arg("--dry-run");
    }
    if let Some(target) = target {
        command.arg(target);
    }
    CliRun::from(&command.output().expect("migration command"))
}

/// Omitted-target discovery reaches the same complete operation, and generated
/// shell completions expose both nested command words. §FS-rhei-migrate.1 §FS-rhei-migrate.6
#[test]
fn omitted_target_and_shell_completion_discover_export_prior_migration() {
    let (dir, plan) = setup("export-prior-discovery", "completed");
    let before = fs::read(&plan).unwrap();
    let mut preview_command = rhei_command(dir.join(".home"));
    preview_command.current_dir(&dir).arg("migrate").arg("export-priors").arg("--dry-run");
    let preview = CliRun::from(&preview_command.output().expect("discovered preview"));
    assert_success(&preview);
    assert!(preview.stdout.contains("Would add Task 1 to Task plan.2"));
    assert_eq!(fs::read(&plan).unwrap(), before);

    let mut completions = rhei_command(dir.join(".home"));
    completions.arg("completions").arg("bash");
    let completions = CliRun::from(&completions.output().expect("bash completions"));
    assert_success(&completions);
    assert!(completions.stdout.contains("migrate"), "{}", completions.stdout);
    assert!(completions.stdout.contains("export-priors"), "{}", completions.stdout);
}

/// The authored edge does not bypass ordinary readiness: nonterminal and
/// cancelled producers still leave the consumer blocked. §FS-rhei-migrate.2.1 §FS-rhei-migrate.5
#[test]
fn migrated_consumer_remains_blocked_by_nonterminal_or_cancelled_producer() {
    for (suffix, state) in [("pending", "pending"), ("cancelled", "cancelled")] {
        let (dir, plan) = setup(&format!("export-prior-{suffix}"), state);
        assert_success(&migrate(&dir.join(".home"), Some(&plan), false));
        let listed = run_cli_without_machine("list", &plan, &["--ready", "--json"]);
        assert_success(&listed);
        let tasks: serde_json::Value = serde_json::from_str(&listed.stdout).expect("list JSON");
        assert!(
            tasks.as_array().unwrap().iter().all(|task| task["id"] != "plan.2"),
            "consumer became ready behind {state}: {}",
            listed.stdout
        );
    }
}

/// Migration repairs authored ordering but neither synthesizes an export nor
/// weakens the pre-spawn nonblank check. §FS-rhei-migrate.1.2 §FS-rhei-migrate.5
#[test]
fn migrated_consumer_still_refuses_missing_or_blank_export_content() {
    for (suffix, body) in [("missing", None), ("blank", Some(" \n\t"))] {
        let (dir, plan) = setup(&format!("export-prior-{suffix}-content"), "completed");
        let agent = write_python_agent(&dir, "consumer.py", AGENT);
        write_mock_agent_settings(&dir, &agent);
        if let Some(body) = body {
            let export = dir.join("runtime/exports/plan.1/x.md");
            fs::create_dir_all(export.parent().unwrap()).unwrap();
            fs::write(export, body).unwrap();
        }
        assert_success(&migrate(&dir.join(".home"), Some(&plan), false));
        let run = run_cli_without_machine("run", &plan, &["--no-tui", "--no-callbacks"]);
        assert!(!run.status.success(), "{suffix} export unexpectedly ran");
        let output = format!("{}{}", run.stdout, run.stderr);
        assert!(output.contains("missing or blank consumed exports"), "{output}");
        assert!(!dir.join("runtime/spawned-plan.2.txt").exists());
    }
}
