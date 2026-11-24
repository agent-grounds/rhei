//! Fixtures and the one runner for `rhei gate`: a machine with the three kinds
//! of gate a brief comes from — a request the state before writes, static
//! questions, and no block at all — and a plan whose tickets reach each.
//! §FS-rhei-gate-brief §FS-rhei-gate-cmd

use std::path::{Path, PathBuf};

use super::*;

/// `review` reads a request, `vetting` asks static questions, `parked` has no
/// `gate:` block. §FS-rhei-gate-brief.1
pub const GATE_MACHINE: &str = r#"name: gate-e2e
version: 1
states:
  todo:
    initial: true
  drafting:
    description: The agent drafts the verdict
  review:
    gating: true
    description: A person rules on the draft
    gate:
      title: "Rule on {task_id}"
      display: runtime/drafts/{task_id}.md
      request: runtime/gates/{task_id}.request.json
      routes:
        - { to: completed, when: { approve: true } }
        - { to: drafting, when: { approve: false } }
  vetting:
    gating: true
    description: A person vets the plan
    gate:
      summary: "{task_title}, in {plan_title}"
      questions:
        - { id: approve, kind: boolean, prompt: "Go ahead?" }
        - { id: note, kind: text, prompt: "Anything to respect?", required: false }
      routes:
        - { to: completed, when: { approve: true } }
        - { to: drafting }
  parked:
    gating: true
    description: Waiting on something outside
  completed:
    final: true
    description: Done and shipped
  cancelled:
    final: true
transitions:
  - {from: todo, to: drafting}
  - {from: drafting, to: review}
  - {from: drafting, to: vetting}
  - {from: drafting, to: parked}
  - {from: review, to: completed}
  - {from: review, to: drafting}
  - {from: vetting, to: completed}
  - {from: vetting, to: drafting}
  - {from: parked, to: drafting}
  - {from: parked, to: completed}
  - {from: "*", to: cancelled}
"#;

/// Ticket 4 follows ticket 1, so ticket 1's gate holds it. §FS-rhei-gate-cmd.4
pub const GATE_PLAN: &str = r#"# Rhei: Verdict

## Tasks

### Task 1: Draft the verdict
**State:** drafting

### Task 2: Vet the plan
**State:** drafting

### Task 3: Park me
**State:** drafting

### Task 4: Follow the verdict
**State:** todo
**Prior:** 1
"#;

/// A request `review` accepts for ticket 1: one required question the routes
/// read, one optional, and an attachment that is not there.
pub const GOOD_REQUEST: &str = r#"{
  "version": 1,
  "title": "Rule on the draft",
  "summary": "The draft is ready.",
  "attachments": ["runtime/notes.md"],
  "questions": [
    { "id": "approve", "kind": "boolean", "prompt": "Ship it?" },
    { "id": "note", "kind": "text", "prompt": "Anything else?", "required": false }
  ]
}"#;

pub struct GateFixture {
    pub dir: TestDir,
    pub home: PathBuf,
}

impl GateFixture {
    pub fn root(&self) -> &Path {
        &self.dir
    }

    /// `rhei <args>` run from the plan's directory, so a bare ticket id names it.
    pub fn rhei(&self, args: &[&str]) -> CliRun {
        let output = rhei_command(&self.home)
            .current_dir(self.root())
            .args(args)
            .output()
            .expect("rhei should execute");
        CliRun::from(&output)
    }

    pub fn write(&self, relative: &str, contents: &str) {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().expect("a fixture file has a parent"))
            .expect("fixture directory");
        fs::write(path, contents).expect("fixture file should be written");
    }

    /// Everything `review` needs to be entered by `ticket`.
    pub fn prepare_review(&self, ticket: &str, request: &str) {
        self.write(&format!("runtime/drafts/{ticket}.md"), "# The draft\n");
        self.write(&format!("runtime/gates/{ticket}.request.json"), request);
    }

    pub fn transition(&self, ticket: &str, from: &str, to: &str) -> CliRun {
        self.rhei(&["transition", ticket, "--from", from, "--to", to])
    }

    pub fn enter(&self, ticket: &str, gate: &str) {
        assert_success(&self.transition(ticket, "drafting", gate));
    }

    /// `rhei gate <args> --json`, parsed.
    pub fn gate_json(&self, args: &[&str]) -> serde_json::Value {
        let mut all = vec!["gate"];
        all.extend_from_slice(args);
        all.push("--json");
        let result = self.rhei(&all);
        assert_success(&result);
        serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
            panic!("rhei gate {args:?} --json is one JSON object: {err}\n{}", result.stdout)
        })
    }

    /// The ticket's state, as `rhei list` reports it.
    pub fn state_of(&self, ticket: &str) -> String {
        let result = self.rhei(&["list", "--json"]);
        assert_success(&result);
        let listed: serde_json::Value = serde_json::from_str(&result.stdout).expect("list JSON");
        let tasks = listed
            .as_array()
            .cloned()
            .or_else(|| listed["tasks"].as_array().cloned())
            .expect("tasks");
        tasks
            .iter()
            .find(|task| task["id"] == ticket)
            .and_then(|task| task["state"].as_str())
            .unwrap_or_else(|| panic!("{ticket} is listed: {listed:#}"))
            .to_string()
    }
}

pub fn gate_fixture(prefix: &str) -> GateFixture {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "verdict.rhei.md", GATE_PLAN);
    write_fixture_file(&dir, "states.yaml", GATE_MACHINE);
    let home = dir.join(".home");
    GateFixture { dir, home }
}

/// The top-level keys of a pretty-printed JSON object, in the order printed.
pub fn top_level_keys(pretty: &str) -> Vec<String> {
    pretty
        .lines()
        .filter_map(|line| line.strip_prefix("  \""))
        .filter_map(|rest| rest.split_once("\":").map(|(key, _)| key.to_string()))
        .collect()
}

/// Validate `instance` against a schema `rhei gate schema` printed.
pub fn assert_matches_schema(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
    label: &str,
) {
    let mut schema = schema.clone();
    // The id is a bare token, not a retrievable URI; nothing refers to it.
    schema.as_object_mut().expect("schema object").remove("$id");
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .unwrap_or_else(|err| panic!("{label} compiles: {err}"));
    if let Err(errors) = compiled.validate(instance) {
        let messages = errors.map(|error| error.to_string()).collect::<Vec<_>>().join("\n");
        panic!("{label} did not validate:\n{messages}\ninstance:\n{instance:#}");
    };
}

pub fn assert_violates_schema(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
    label: &str,
) {
    let mut schema = schema.clone();
    schema.as_object_mut().expect("schema object").remove("$id");
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .unwrap_or_else(|err| panic!("{label} compiles: {err}"));
    assert!(compiled.validate(instance).is_err(), "{label} should not validate:\n{instance:#}");
}
