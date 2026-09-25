use std::fs;
use std::process::Command;

use super::*;

/// The prompt a session report shows is the prompt the worker was given:
/// recorded beside the log when the spawn opens it, byte for byte, and never
/// rebuilt from the task body the worker is about to change.
// §FS-rhei-session-reports.1.1 §FS-rhei-session-reports.2
#[test]
fn the_report_shows_the_prompt_the_worker_received() {
    let dir = unique_temp_dir("session-report-prompt");
    let git = Command::new("git").arg("init").arg("-q").current_dir(&dir).status();
    if !git.map(|status| status.success()).unwrap_or(false) {
        eprintln!("skipping: git unavailable");
        return;
    }
    let workspace = dir.join("ws");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace");

    // The worker keeps the prompt it read, then edits its own task body the
    // way `Leaving a trail` invites — which a rebuilt prompt would show.
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"prompt = agent_prompt()
root = pathlib.Path(env('RHEI_ROOT'))
write(root / 'runtime' / 'received-prompt.md', prompt)
task_file = root / 'tasks' / '01-work.md'
write(task_file, task_file.read_text() + '\nVisit note: added after the prompt was read.\n')
result('## Result\n\nRecorded the prompt.\n')
"#,
    );
    let agent_command =
        serde_json::to_string(&vec![python_command().to_string(), agent.display().to_string()])
            .expect("serialize mock agent command");
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{
    "mock": {{ "command": {agent_command}, "stdin_prompt": true, "timeout": "30s" }}
  }}
}}"#
        ),
    )
    .expect("write settings");
    fs::write(
        workspace.join("index.rhei.md"),
        "# Rhei: Prompt Record\n**States:** prompt-record\n\n## Overview\n\nOne agent task.\n",
    )
    .expect("write index");
    fs::write(
        workspace.join("tasks/01-work.md"),
        "### Task 1: Record the prompt\n**State:** work\n\nPROMPT-MARKER: the task body as delivered.\n",
    )
    .expect("write task file");
    fs::write(
        workspace.join("states.yaml"),
        "name: prompt-record\nversion: 1\nstates:\n  work:\n    initial: true\n    agent: mock\n\
         \x20   instructions: Record the prompt.\n  completed:\n    final: true\n\
         transitions:\n  - from: work\n    to: completed\n",
    )
    .expect("write state machine");

    let output = rhei_command(dir.join(".home"))
        .current_dir(&dir)
        .args(["run", "ws", "--no-tui", "--no-callbacks"])
        .output()
        .expect("run the workspace");
    assert!(
        output.status.success(),
        "run failed\nstdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );

    let received =
        fs::read_to_string(workspace.join("runtime/received-prompt.md")).expect("worker's prompt");
    assert!(received.contains("PROMPT-MARKER"), "the worker read its task: {received}");
    let logs = workspace.join("runtime/logs");
    let records: Vec<_> = fs::read_dir(workspace.join("runtime/prompts"))
        .expect("prompts directory")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    assert_eq!(records.len(), 1, "one prompt record per log: {records:?}");
    let record = fs::read_to_string(&records[0]).expect("prompt record");
    assert_eq!(record, received, "the record is the prompt the worker received");

    // The report embeds the record, not a rebuild that would already carry
    // the note the worker appended after reading it.
    let stem = records[0].file_stem().expect("record name").to_string_lossy().into_owned();
    assert!(logs.join(format!("{stem}.log")).is_file(), "the record shares its log's stem");
    let report = fs::read_to_string(workspace.join(format!("runtime/reports/{stem}.md")))
        .expect("session report");
    assert!(report.contains("## Prompt"), "report has a prompt section:\n{report}");
    assert!(report.contains("PROMPT-MARKER: the task body as delivered."), "report:\n{report}");
    assert!(!report.contains("Visit note: added after"), "no rebuilt prompt:\n{report}");
}
