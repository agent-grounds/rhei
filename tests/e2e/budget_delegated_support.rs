//! The fixture world the delegated-ceiling scenarios share: a machine file and
//! a project file side by side, and the paths a delegated report must name.
//! §FS-rhei-budgets.2 §FS-rhei-agents.1.1.1

use std::fs;
use std::path::{Path, PathBuf};

use super::budget_support::*;
use super::*;

/// The day every window scenario here runs on, so a raise and the halt before
/// it are counted against the same window. §FS-rhei-budgets.3.3.1
pub(super) const DAY: &str = "2026-10-06T12:00:00Z";

/// The ping-pong machine with one profile, which declares `transition_limit`
/// only where `limit` names one.
pub(super) fn profile_machine(limit: Option<u32>) -> String {
    let limit = limit.map(|n| format!("\n    transition_limit: {n}")).unwrap_or_default();
    format!(
        r#"name: budget-delegated
version: 1
states:
  work:
    description: Do a round of work
    agent: mock
    agent_timeout: 30s
    outputs:
      - name: work
        path: runtime/work.md
  review:
    description: Send it back for another round
    agent: mock
    agent_timeout: 30s
    outputs:
      - name: review
        path: runtime/review.md
  cancelled:
    description: Stop
    final: true
profiles:
  loop:
    initial: work
    allowed: [work, review, cancelled]{limit}
node_policy:
  root: loop
  default: loop
transitions:
  - {{ from: work, to: review, description: Round done }}
  - {{ from: review, to: work, description: Another round }}
  - {{ from: work, to: cancelled, description: Stop }}
  - {{ from: review, to: cancelled, description: Stop }}
"#
    )
}

/// The `defaults` addition of a machine that delegates.
pub(super) const DELEGATES: &str = r#", "clamp_projects": false"#;

/// A workspace whose machine carries `machine_extra` in its `defaults` and
/// whose project checks in `project` at `home` — the current
/// `.agent-grounds/rhei` or the deprecated `.agents/rhei`.
pub(super) fn workspace(
    prefix: &str,
    machine: &str,
    machine_extra: &str,
    home: &str,
    project: &str,
) -> (TestDir, PathBuf, PathBuf) {
    let (dir, plan, states) = setup(prefix, machine, machine_extra);
    write_project_settings(&dir, home, project);
    (dir, plan, states)
}

pub(super) fn write_project_settings(root: &Path, home: &str, body: &str) {
    let dir = root.join(home);
    fs::create_dir_all(&dir).expect("create project settings home");
    fs::write(dir.join("settings.json"), body).expect("write project settings");
}

/// The machine settings file as the loader opens it: `$HOME` joined with the
/// fixed relative path, the same join the product makes. §FS-rhei-budgets.8
pub(super) fn machine_file(root: &Path) -> PathBuf {
    home_for(root).join(".config/rhei/settings.json")
}

pub(super) fn project_file(root: &Path, home: &str) -> PathBuf {
    root.join(home).join("settings.json")
}

/// Both spellings a temporary path may be printed in: as the test wrote it,
/// and canonical (macOS reaches the same directory through `/private/var`).
pub(super) fn spellings(path: &Path) -> Vec<String> {
    let mut all = vec![path.display().to_string()];
    if let Ok(canonical) = fs::canonicalize(path) {
        let canonical = canonical.display().to_string();
        if !all.contains(&canonical) {
            all.push(canonical);
        }
    }
    all
}

/// Assert `text` holds `before<path>after` for some spelling of each path, so
/// the line is pinned exactly but the temporary directory's spelling is not.
pub(super) fn assert_line(
    text: &str,
    line: impl Fn(&str, &str) -> String,
    p: &Path,
    m: &Path,
    why: &str,
) {
    let found =
        spellings(p).iter().any(|p| spellings(m).iter().any(|m| text.contains(&line(p, m))));
    assert!(
        found,
        "{why}\nexpected: {}\ngot:\n{text}",
        line(&p.display().to_string(), &m.display().to_string())
    );
}

pub(super) fn combined(result: &CliRun) -> String {
    format!("{}{}", result.stdout, result.stderr)
}

/// `rhei budget <args…>` against this workspace at `now`.
pub(super) fn budget(plan: &Path, args: &[&str], now: Option<&str>) -> CliRun {
    let root = plan.parent().expect("plan has a parent");
    let mut cmd = rhei_command(home_for(root));
    cmd.arg("budget").arg(args[0]).arg(plan).args(&args[1..]);
    if let Some(instant) = now {
        cmd.env("RHEI_BUDGET_NOW", instant);
    }
    CliRun::from(&cmd.output().expect("rhei budget should run"))
}

pub(super) fn budget_json(plan: &Path, now: Option<&str>) -> serde_json::Value {
    let shown = budget(plan, &["show", "--format", "json"], now);
    assert_success(&shown);
    serde_json::from_str(&shown.stdout).expect("budget show --format json is JSON")
}

/// Assert the halt's remedy sends its reader to the machine settings file by
/// the path the loader opened — `$HOME` joined with the fixed relative path —
/// rather than by its role. §FS-rhei-budgets.8
pub(super) fn assert_remedy_names_machine_file(result: &CliRun, root: &Path, key: &str) {
    let machine = home_for(root).join(".config/rhei/settings.json");
    let canonical = fs::canonicalize(&machine).unwrap_or_else(|_| machine.clone());
    let combined = format!("{}{}", result.stdout, result.stderr);
    let found = [machine, canonical]
        .iter()
        .any(|path| combined.contains(&format!("set `defaults.{key}` in {}", path.display())));
    assert!(
        found,
        "expected the remedy to name the machine settings file by its path; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout, result.stderr
    );
}
