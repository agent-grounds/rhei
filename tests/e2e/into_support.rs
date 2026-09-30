//! Fixtures for `rhei instantiate --into`: a host rhei with a machine of its
//! own, and the templates that get placed into it.
//!
//! Every template here is a *valid standalone template* as well as something to
//! place, because that is the property the union rests on — a template works at
//! any level, so the same bytes lay a rhei with `--output` and join one with
//! `--into`. §FS-rhei-library.6

use std::path::{Path, PathBuf};

use super::*;

/// Run the built binary with a home of its own, so the user template tier only
/// holds what a test put there.
pub fn run_into(args: &[&str], dir: &Path) -> CliRun {
    let output = rhei_command(dir.join(".home"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

/// The host's machine: three states, one profile, one kind, and a wildcard
/// cancel edge written *unscoped*, which is the form §FS-rhei-library.3.2 says
/// the host keeps and the union lets span what it takes in.
pub const HOST_MACHINE: &str = r#"name: host
version: 1
states:
  pending:
    description: Ready for work
    instructions: |
      Do the work and move on.
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
profiles:
  host:
    initial: pending
    allowed: [pending, completed, cancelled]
node_policy:
  root: host
  default: host
  by_type:
    task: host
"#;

pub const HOST_INDEX: &str = r#"# Rhei: Release
**States:** host

---
structure:
  maxLevels: 2
  nodeKinds:
  - task
---

## Overview

The plan a template gets placed into.
"#;

pub const HOST_TICKET: &str = r#"### Task ticket: Ship it
**State:** pending

The host's own ticket, whose bytes a placement must not touch.
"#;

/// A directory-workspace host rhei with a machine of its own and one ticket.
/// Returns (temp dir, the rhei root).
pub fn host_workspace(prefix: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let root = dir.join("release");
    std::fs::create_dir_all(root.join("tasks")).expect("create host workspace");
    write_fixture_file(&root, "index.rhei.md", HOST_INDEX);
    write_fixture_file(&root, "states.yaml", HOST_MACHINE);
    write_fixture_file(&root.join("tasks"), "001-ticket.md", HOST_TICKET);
    (dir, root)
}

/// A single-file host rhei with a sibling machine, for the `## Tasks` placement
/// §FS-rhei-new.3.1 specifies.
pub fn host_single_file(prefix: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(
        &dir,
        "release.rhei.md",
        "# Rhei: Release\n**States:** host\n\n## Tasks\n\n### Task ticket: Ship it\n**State:** pending\n",
    );
    write_fixture_file(&dir, "states.yaml", HOST_MACHINE);
    let root = dir.to_path_buf();
    (dir, root)
}

/// The template that gets placed: two states of its own, its own profile, its
/// own node kind, its own *scoped-on-write* wildcard, and one ticket.
///
/// The `step` kind rather than `task` is deliberate — a template routes its
/// tickets through a kind of its own, and a `task`-kind ticket that fell into
/// the host's default lane would validate and be wrong.
/// §FS-rhei-library.3.3
pub const REVIEW_TEMPLATE_MACHINE: &str = r#"name: review-loop
version: 1
states:
  review:
    description: Read the change and write findings
    instructions: |
      Review {{change_ref}} and write what you found.
  decide:
    description: Decide what to do about the findings
    instructions: |
      Read the findings and decide.
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: review
    to: decide
  - from: decide
    to: completed
  - from: "*"
    to: cancelled
profiles:
  review-loop:
    initial: review
    allowed: [review, decide, completed, cancelled]
node_policy:
  root: review-loop
  default: review-loop
  by_type:
    step: review-loop
"#;

/// Write the `review-loop` template into `<dir>/.agent-grounds/rhei/templates`.
pub fn write_review_template(dir: &Path) -> PathBuf {
    let template = dir.join(".agent-grounds/rhei/templates/review-loop");
    std::fs::create_dir_all(template.join("tasks")).expect("create template");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: review-loop\nversion: 1.0.0\ndescription: Review a change and decide\ninputs:\n  - name: change_ref\n    description: The change to review\n    type: string\n",
    );
    write_fixture_file(&template, "states.yaml", REVIEW_TEMPLATE_MACHINE);
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Review {{change_ref}}\n**States:** review-loop\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - step\n---\n\n## Overview\n\nA review loop.\n",
    );
    write_fixture_file(
        &template,
        "tasks/001-coordinate.md",
        "### Step coordinate: Coordinate review of {{change_ref}}\n**State:** review\n\nReview {{change_ref}}.\n",
    );
    // A sibling `**Prior:**` naming a *template* task: what placement has to
    // rewrite to the placed id. §FS-rhei-library.4
    write_fixture_file(
        &template,
        "tasks/002-record.md",
        "### Step record: Record what the review decided\n**State:** review\n**Prior:** coordinate\n\nRecord the decision.\n",
    );
    template
}

/// Read a file the host owns, for a byte-for-byte comparison across a command.
pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

pub const PROJECT_INDEX: &str = r#"# Panta: Scratch

## Overview

The project whose member a template gets placed into.
"#;

/// Make the fixture directory a Panta project, so the host rhei beside it is a
/// *member*: the settings root a member resolves is the project's and never its
/// own, and the terms it is correct in are the project's.
/// §FS-rhei-templates.6.2 §AR-rhei-panta.1
pub fn make_project(dir: &Path) {
    write_fixture_file(dir, "index.panta.md", PROJECT_INDEX);
}

/// Write a settings file into a rhei home under `base`, creating the home.
pub fn write_settings(base: &Path, contents: &str) -> PathBuf {
    let home = base.join(".agent-grounds/rhei");
    std::fs::create_dir_all(&home).expect("create the rhei home");
    write_fixture_file(&home, "settings.json", contents)
}

/// A template whose one state targets an agent it does *not* ship: whoever
/// validates the union has to resolve the agent from the settings the rhei
/// actually runs under.
pub fn write_borrowed_agent_template(dir: &Path) -> PathBuf {
    let template = dir.join(".agent-grounds/rhei/templates/polish");
    std::fs::create_dir_all(template.join("tasks")).expect("create template");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: polish\nversion: 1.0.0\ndescription: One state on an agent the project configures\n",
    );
    write_fixture_file(
        &template,
        "states.yaml",
        "name: polish\nversion: 1\nstates:\n  polish:\n    description: Polish the change\n    target: borrowed[x]:openai:gpt-5.5\n    instructions: |\n      Polish it.\n  completed:\n    final: true\n    description: Done\n  cancelled:\n    final: true\n    description: Abandoned\ntransitions:\n  - from: polish\n    to: completed\n  - from: \"*\"\n    to: cancelled\nprofiles:\n  polish:\n    initial: polish\n    allowed: [polish, completed, cancelled]\nnode_policy:\n  root: polish\n  default: polish\n  by_type:\n    polish: polish\n",
    );
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Polish\n**States:** polish\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - polish\n---\n\n## Overview\n\nA polish step.\n",
    );
    template
}

/// The settings that define the agent `write_borrowed_agent_template` borrows.
pub const BORROWED_AGENT_SETTINGS: &str = r#"{
  "agents": {
    "borrowed": {
      "command": ["true"],
      "model_flag": "--model",
      "stdin_prompt": true,
      "timeout": "30m",
      "modes": { "x": ["--flag"] }
    }
  }
}
"#;
