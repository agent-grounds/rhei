//! Where the task tooling fields and `withhold_task_tooling` show: written by
//! `rhei new`, exported by `rhei render`, and printed by `rhei states`.
//! agent-grounds/rhei#475. §FS-rhei-new.1.3 §FS-rhei-render.3.1
//! §FS-rhei-states-cmd.4 §FS-rhei-states-cmd.5
// §FS-rhei-task-tooling.7 §FS-rhei-task-tooling.8

use std::fs;
use std::path::Path;

use super::new_tests::{assert_failure, new_run, project_with_rhei};
use super::*;

fn write_registry(dir: &Path) {
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    let settings = serde_json::json!({
        "mcp_servers": {
            "thunderbird-mail": { "command": ["mcp-thunderbird-mail"] },
            "grafana": { "url": "http://127.0.0.1:9/sse", "transport": "sse" }
        },
        "skills": { "release-notes": { "path": "./skills/release-notes" } }
    });
    write_fixture_file(&settings_dir, "settings.json", &settings.to_string());
}

/// `rhei new` writes both fields last, each on one line in the order the flags
/// were given, and an entry keeps its ` (optional)` marker.
// §FS-rhei-task-tooling.8
#[test]
fn new_writes_task_tooling_last_in_the_order_given() {
    let dir = project_with_rhei("new-task-tooling");
    write_registry(&dir);

    let created = new_run(
        &[
            "new",
            "Summarise the release thread",
            "--under",
            "auth",
            "--assignee",
            "manual",
            "--mcp-server",
            "thunderbird-mail",
            "--mcp-server",
            "grafana (optional)",
            "--skill",
            "release-notes",
        ],
        &dir,
    );

    assert_success(&created);
    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("created plan");
    assert!(
        plan.contains(
            "**Assignee:** manual\n\
             **MCP servers:** thunderbird-mail, grafana (optional)\n\
             **Skills:** release-notes\n"
        ),
        "got:\n{plan}"
    );
}

/// An id the registry does not hold is refused as an argument, before the plan
/// is touched.
// §FS-rhei-task-tooling.8
#[test]
fn new_refuses_an_unknown_task_server_before_writing_anything() {
    let dir = project_with_rhei("new-task-tooling-unknown");
    write_registry(&dir);
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("plan before");

    let refused =
        new_run(&["new", "Mail", "--under", "auth", "--mcp-server", "thunderbird-mial"], &dir);

    assert_failure(&refused, "thunderbird-mial");
    let after = fs::read_to_string(dir.join("auth.rhei.md")).expect("plan after");
    assert_eq!(after, before, "nothing may be written for a refused id");
}

/// A description line that opens with either field would author it, so it is
/// refused like every other metadata marker.
// §FS-rhei-task-tooling.8 §FS-rhei-new.3.4
#[test]
fn new_refuses_a_description_line_opening_with_a_task_tooling_field() {
    let dir = project_with_rhei("new-task-tooling-description");
    write_registry(&dir);
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("plan before");

    for line in ["**MCP servers:** thunderbird-mail", "**Skills:** release-notes"] {
        let refused = new_run(&["new", "Mail", "--under", "auth", "--description", line], &dir);
        assert_failure(&refused, "would be read as plan structure");
    }
    let after = fs::read_to_string(dir.join("auth.rhei.md")).expect("plan after");
    assert_eq!(after, before, "nothing may be written for a refused description");
}

const TOOLED_PLAN: &str = "# Rhei: Inbox

## Tasks

### Task 1: Summarise this week's release thread from the mail
**State:** pending
**MCP servers:** thunderbird-mail, grafana (optional)
**Skills:** release-notes

### Task 2: Tidy the changelog
**State:** pending
";

/// JSON gives every task both arrays, empty when the field is absent; the
/// GitHub form keeps the lines, and `--no-metadata` drops them.
// §FS-rhei-task-tooling.7
#[test]
fn render_carries_each_tasks_tooling_and_no_metadata_hides_it() {
    let dir = unique_temp_dir("render-task-tooling");
    let plan = write_fixture_file(&dir, "plan.rhei.md", TOOLED_PLAN);

    let json = run_cli_without_machine("render", &plan, &["--format", "json"]);
    assert_success(&json);
    let rendered: serde_json::Value = serde_json::from_str(&json.stdout).expect("rendered JSON");
    assert_eq!(
        rendered["tasks"][0]["mcp_servers"],
        serde_json::json!([
            { "id": "thunderbird-mail", "optional": false },
            { "id": "grafana", "optional": true }
        ])
    );
    assert_eq!(
        rendered["tasks"][0]["skills"],
        serde_json::json!([{ "id": "release-notes", "optional": false }])
    );
    assert_eq!(rendered["tasks"][1]["mcp_servers"], serde_json::json!([]));
    assert_eq!(rendered["tasks"][1]["skills"], serde_json::json!([]));

    let github = run_cli_without_machine("render", &plan, &["--format", "github"]);
    assert_success(&github);
    assert!(
        github.stdout.contains(
            "**MCP servers:** thunderbird-mail, grafana (optional)\n**Skills:** release-notes"
        ),
        "got:\n{}",
        github.stdout
    );
    let hidden = run_cli_without_machine("render", &plan, &["--format", "github", "--no-metadata"]);
    assert_success(&hidden);
    assert!(!hidden.stdout.contains("**MCP servers:**"), "got:\n{}", hidden.stdout);
    assert!(!hidden.stdout.contains("**Skills:**"), "got:\n{}", hidden.stdout);
}

const WITHHOLDING_MACHINE: &str = "name: inbox-review
version: 1.0.0
states:
  pending:
    initial: true
    agent: claude-code
    agent_timeout: 30s
  review:
    agent: claude-code
    agent_timeout: 30s
    withhold_task_tooling: true
  completed:
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
";

/// Only the state that authors the field shows it: a text line, and a JSON
/// member a state without the field does not carry.
// §FS-rhei-task-tooling.7
#[test]
fn states_shows_a_withholding_state_and_leaves_the_others_unchanged() {
    let dir = unique_temp_dir("states-task-tooling");
    let plan = write_fixture_file(&dir, "plan.rhei.md", "# Rhei: Inbox\n\n## Tasks\n");
    let machine = write_fixture_file(&dir, "states.yaml", WITHHOLDING_MACHINE);

    let text = run_cli("states", &plan, &machine, &[]);
    assert_success(&text);
    assert_eq!(
        text.stdout.matches("Task tooling: withheld").count(),
        1,
        "only `review` withholds; got:\n{}",
        text.stdout
    );

    let json = run_cli("states", &plan, &machine, &["--json"]);
    assert_success(&json);
    let inspected: serde_json::Value = serde_json::from_str(&json.stdout).expect("states JSON");
    let state = |name: &str| {
        inspected["states"]
            .as_array()
            .expect("states array")
            .iter()
            .find(|state| state["name"] == name)
            .unwrap_or_else(|| panic!("no state '{name}'"))
            .clone()
    };
    assert_eq!(state("review")["withhold_task_tooling"], serde_json::json!(true));
    assert!(
        state("pending").get("withhold_task_tooling").is_none(),
        "a state that does not author the field must not carry the member: {}",
        state("pending")
    );
}
