//! Basin reset fixtures use the same isolated CLI harness as the reset-remedy
//! scenarios. The manifest owns basin metadata. §FS-rhei-reset.2 §FS-rhei-panta.6

use std::fs;

use serde_yaml::Value;

use super::reset_remedy_support::RemedyWorld;

const MACHINE: &str = r#"name: reset-basin
version: 1
states:
  open:
    initial: true
    description: Open
  work:
    description: Work
    visits: 1
  done:
    final: true
    description: Done
transitions:
  - {from: open, to: work}
  - {from: work, to: done}
"#;

pub(super) const AUTHORED_BLOCK: &str =
    "---\nmetadata:\n  tasks:\n    1:\n      custom: {keep: 'exact spelling'}\n---\n";

pub(super) fn project(name: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "project");
    world.write("index.panta.md", "# Panta: Reset basin\n\nProject prose.\n");
    world.write("states.yaml", MACHINE);
    world.write("basin/01-first.md", "### Task 1: First\n**State:** open\n");
    world.write("ws/index.rhei.md", "# Rhei: Workspace control\n");
    world.write(
        "ws/tasks/01-first.md",
        &format!("{AUTHORED_BLOCK}\n### Task 1: Control\n**State:** open\n"),
    );
    world
}

pub(super) fn read(world: &RemedyWorld, path: &str) -> String {
    fs::read_to_string(world.root.join(path)).expect("read fixture")
}

/// Read stored frontmatter directly: list's author-metadata view intentionally
/// omits the runtime keys this contract checks. §FS-rhei-transitions.2.5
pub(super) fn frontmatter(world: &RemedyWorld, path: &str) -> Value {
    let text = read(world, path);
    let yaml = text.split("---").nth(1).unwrap_or("{}");
    serde_yaml::from_str(yaml).expect("valid frontmatter")
}

pub(super) fn reset(world: &RemedyWorld, narrow: bool) {
    let args: &[&str] = if narrow { &["--rhei", "basin", "--yes"] } else { &["--yes"] };
    world.step("reset", args);
}

/// All three reset-cleared keys, with valid provider-wait and supervision
/// shapes, on a ticket that need not have moved. §FS-rhei-transitions.2.5
pub(super) fn runtime_keys() -> Value {
    serde_yaml::from_str(
        r#"stateVisits:
  work: 1
providerLimits:
  work:
    identity:
      agent: codex
      provider: openai
    signal: "You've hit your session limit"
    observedAt: "2026-01-01T00:00:00Z"
    nextAttemptAt: "2026-01-01T01:00:00Z"
supervision:
  phase: released
"#,
    )
    .expect("valid runtime seeds")
}

pub(super) fn write_manifest(world: &RemedyWorld, metadata: &Value) {
    let mut frontmatter = serde_yaml::Mapping::new();
    frontmatter.insert(Value::String("metadata".into()), metadata.clone());
    world.write(
        "index.panta.md",
        &format!(
            "# Panta: Reset basin\n---\n{}---\n\nProject prose.\n",
            serde_yaml::to_string(&frontmatter).expect("serialize frontmatter")
        ),
    );
}

/// The preserved half of the key register plus author data at several levels;
/// a retirement is project bookkeeping rather than a live task's runtime key.
/// §FS-rhei-reset.2 §FS-rhei-transitions.2.5
pub(super) fn preserved_metadata() -> Value {
    serde_yaml::from_str(
        r#"customProject: {keep: true}
retiredTickets:
  basin.9:
    budgetTicketId: 7d0c3f1e-0000-4000-8000-000000000009
tasks:
  basin.1:
    budgetTicketId: 7d0c3f1e-0000-4000-8000-000000000001
    pollNextAttemptAt:
      work: "2026-01-01T01:00:00Z"
    priority: high
    custom: {nested: [one, two]}
"#,
    )
    .expect("valid preserved metadata")
}

pub(super) fn seed_metadata(world: &RemedyWorld) -> Value {
    let expected = preserved_metadata();
    let mut seeded = expected.clone();
    for (key, value) in runtime_keys().as_mapping().expect("runtime keys") {
        seeded["tasks"]["basin.1"][key.clone()] = value.clone();
    }
    // A never-moved descendant and sibling must have runtime-only maps pruned.
    seeded["tasks"]["basin.1.1"] = runtime_keys();
    seeded["tasks"]["basin.2"] = runtime_keys();
    write_manifest(world, &seeded);
    world.write(
        "basin/01-first.md",
        "### Task 1: First\n**State:** open\n\n#### Task 1.1: Child\n**State:** open\n",
    );
    world.write("basin/02-second.md", "### Task 2: Never moved\n**State:** open\n");
    expected
}
