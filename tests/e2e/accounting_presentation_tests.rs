//! Black-box proof that every run-summary surface exposes the normalized cache
//! dimensions without adding them to the inclusive totals again.
//! §FS-rhei-cost-accounting.3.1 §FS-rhei-cost-accounting.5.2
//! §FS-rhei-run-report.2.1 §FS-rhei-run-report.2.2 §FS-rhei-summary.2.3

#[cfg(unix)]
use std::fs;

#[cfg(unix)]
use super::accounting_support::{
    accounting_workspace_with_agent, invocation_records, WORKING_PLAN,
};
#[cfg(unix)]
use super::native_pty_support::open_native_terminal;
#[cfg(unix)]
use super::*;

#[cfg(unix)]
const CACHE_DIMENSION_AGENT: &str = r#"capture = env('RHEI_ACCOUNTING_USAGE_PATH')
if capture:
    append(
        capture,
        '{"schema":"rhei.accounting.usage.v1","usage":'
        '{"input_tokens":1100,"cache_read_input_tokens":700,'
        '"cache_creation_input_tokens":300,"output_tokens":50}}\n',
    )
result('## Result\n\nCache-dimension fixture finished.\n')
"#;

#[cfg(unix)]
const LEGACY_CLAUDE_RECORD: &str = r#"{
  "schema": "rhei.accounting.invocation.v1",
  "invocation_id": "legacy::work::claude-code::visit-1",
  "task_id": "plan.1",
  "state": "work",
  "visit": 1,
  "agent": "claude-code",
  "provider": "anthropic",
  "model": "claude-sonnet-4-6",
  "started_at": "2026-09-06T10:00:00Z",
  "ended_at": "2026-09-06T10:00:01Z",
  "extraction_status": "measured",
  "scope": "aggregate-agent-process",
  "tokens": {
    "total": { "value": 150, "source": "agent-usage-capture" },
    "input": {
      "total": { "value": 100, "source": "agent-usage-capture" },
      "cached_read": { "value": 700, "source": "agent-usage-capture" },
      "cache_write": { "value": 300, "source": "agent-usage-capture" }
    },
    "output": {
      "total": { "value": 50, "source": "agent-usage-capture" },
      "cached_read": { "status": "unsupported" },
      "cache_write": { "status": "unsupported" }
    }
  },
  "pricing": {
    "status": "priced",
    "currency": "USD",
    "amount_micro": 1000,
    "priced_amount_micro": 1000,
    "price_book_id": "builtin-2026-05-20"
  }
}"#;

#[cfg(unix)]
fn run_in_tty(dir: &Path, plan: &Path, machine: &Path) -> CliRun {
    use std::io::Read as _;
    use std::os::fd::OwnedFd;

    let winsize = nix::pty::Winsize { ws_row: 40, ws_col: 160, ws_xpixel: 0, ws_ypixel: 0 };
    // The read below runs to end of file, and a leaked slave would hold that off
    // for as long as the process that took it lives. §REQ-cross-platform.7.2
    let pty = open_native_terminal(&winsize);
    let mut command = rhei_command(dir.join(".home"));
    command
        .env("NO_COLOR", "1")
        .arg("--state-machine")
        .arg(machine)
        .arg("run")
        .arg(plan)
        .arg("--no-tui")
        .arg("--no-callbacks");

    let stdout: OwnedFd = pty.slave.try_clone().expect("clone pty slave");
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().expect("rhei run should start");
    drop(command);
    drop(pty.slave);

    let mut stderr = child.stderr.take().expect("piped stderr");
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).expect("read run stderr");
        bytes
    });
    let mut stdout = Vec::new();
    let mut master = fs::File::from(pty.master);
    let mut chunk = [0_u8; 8_192];
    while let Ok(count) = master.read(&mut chunk) {
        if count == 0 {
            break;
        }
        stdout.extend_from_slice(&chunk[..count]);
    }
    let status = child.wait().expect("wait for rhei run");
    let captured = std::process::Output {
        status,
        stdout,
        stderr: stderr_reader.join().expect("join stderr reader"),
    };
    // The pty puts a carriage return before every newline it echoes, and
    // these assertions are about the table rather than the line discipline.
    CliRun { stdout: super::stdout(&captured).replace('\r', ""), ..CliRun::from(&captured) }
}

#[cfg(unix)]
fn table(text: &str, heading: &str) -> String {
    let start = text.find(heading).unwrap_or_else(|| panic!("missing {heading:?} in:\n{text}"));
    let table = &text[start..];
    table.split_once("\n\n").map_or(table, |(body, _)| body).to_string()
}

#[cfg(unix)]
fn row_labels(table: &str) -> Vec<&str> {
    table
        .lines()
        .skip(2)
        .filter_map(|line| {
            line.strip_prefix("| ").and_then(|line| line.split_once(" |")).map(|p| p.0)
        })
        .collect()
}

/// This is the issue-160 reproducer as a repository test: one current capture
/// and one legacy Claude-shaped record exercise all four public surfaces. The
/// total assertions run before presentation assertions so a cache double-count
/// cannot hide behind the expected renderer failure.
// §FS-rhei-cost-accounting.3.1 §FS-rhei-cost-accounting.5.2
// §FS-rhei-run-report.2.1 §FS-rhei-run-report.2.2 §FS-rhei-summary.2.3
#[cfg(unix)]
#[test]
fn accounting_presentation_every_surface_shows_cache_writes_without_double_counting() {
    let (dir, plan, machine) = accounting_workspace_with_agent(
        "accounting-presentation",
        WORKING_PLAN,
        CACHE_DIMENSION_AGENT,
    );
    let records = dir.join("runtime/accounting/invocations");
    fs::create_dir_all(&records).expect("create invocation store");
    fs::write(records.join("legacy-claude.json"), LEGACY_CLAUDE_RECORD)
        .expect("seed legacy record");

    let run = run_in_tty(&dir, &plan, &machine);
    assert_success(&run);
    let current = invocation_records(&dir)
        .into_iter()
        .find(|record| record.get("token_convention").is_some())
        .expect("the current run wrote one convention-tagged record");
    assert_eq!(
        current["tokens"]["total"]["value"].as_u64(),
        Some(1_150),
        "the run total remains inclusive input plus inclusive output"
    );
    let rollup: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("runtime/accounting/summary.json"))
            .expect("accounting summary"),
    )
    .expect("accounting summary parses");
    assert_eq!(
        rollup["summary"]["total"]["value"].as_u64(),
        Some(2_300),
        "the current and restated legacy records remain inclusive in aggregate"
    );
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    let summary = run_cli("summary", &plan, &machine, &[]);
    assert_success(&summary);

    let accounting = table(&report, "| Accounting (this run) | Value |");
    let task_costs = table(&report, "| Task | Cost | Total |");
    let aggregate = table(&summary.stdout, "| Accounting | Value |");

    assert!(accounting.contains("| total tokens | 1.1k |"), "{accounting}");
    assert!(aggregate.contains("| total tokens | 2.3k |"), "{aggregate}");
    assert!(run.stdout.contains("Total 1.1k"), "{}", run.stdout);

    let expected_rows = [
        "total tokens",
        "input tokens (incl. cache)",
        "input cache read",
        "input cache write",
        "output tokens (incl. cache)",
        "output cache read",
        "output cache write",
    ];
    let mut failures = Vec::new();
    if !row_labels(&accounting).windows(expected_rows.len()).any(|rows| rows == expected_rows) {
        failures.push(format!("durable accounting rows:\n{accounting}"));
    }
    let task_header = "| Task | Cost | Total | Input (incl. cache) | Input cache read | Input cache write | Output (incl. cache) | Output cache read | Output cache write | Coverage |";
    if !task_costs.lines().any(|line| line == task_header)
        || !task_costs.lines().any(|line| line.contains("| 300 |"))
    {
        failures.push(format!("Task Costs columns/value:\n{task_costs}"));
    }
    if !row_labels(&aggregate).windows(expected_rows.len()).any(|rows| rows == expected_rows)
        || !aggregate.contains("| input cache write | 600 |")
    {
        failures.push(format!("aggregate accounting rows:\n{aggregate}"));
    }
    if !run.stdout.contains("In 1.1k (incl. cache: read 700, write 300)")
        || !run.stdout.contains("Out 50 (incl. cache: read -, write -)")
    {
        failures.push(format!("TTY accounting grouping:\n{}", run.stdout));
    }

    assert!(
        failures.is_empty(),
        "cache dimensions are hidden or ambiguously labelled on these surfaces:\n{}",
        failures.join("\n\n")
    );
}
