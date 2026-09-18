use std::fs;
use std::path::{Path, PathBuf};

use super::block_composition_support::{mount_arg, run_compose};
use super::*;

pub(super) struct ExposureFixture {
    pub leaf: PathBuf,
}

fn write_block_file(block: &Path, relative: &str, contents: &str) {
    let path = block.join(relative);
    fs::create_dir_all(path.parent().expect("block file parent"))
        .expect("create block file parent");
    write_fixture_file(block, relative, contents);
}

pub(super) fn write_exposure_leaf(root: &Path, name: &str, internal_state: &str) -> PathBuf {
    let block = root.join(name);
    fs::create_dir_all(&block).expect("create exposure block");
    write_block_file(
        &block,
        "template.yaml",
        &format!(
            r#"name: {name}
version: 1
description: Expose typed fixture identities
ports:
  entry: {internal_state}
  exits:
    done: internal-done
expose:
  states:
    ready: {{ local: {internal_state} }}
  tasks:
    audit: {{ local: audit-internal }}
  settings:
    agents:
      reviewer: {{ local: internal-agent }}
    models:
      careful: {{ local: internal-model }}
    mcp_servers:
      tracker: {{ local: internal-tracker }}
    skills:
      checklist: {{ local: internal-skill }}
"#,
        ),
    );
    write_block_file(
        &block,
        "index.rhei.md",
        &format!("# Rhei: Exposure leaf\n**States:** {name}\n"),
    );
    write_block_file(
        &block,
        "tasks/01-audit.md",
        &format!(
            "### Task audit-internal: Audit\n**State:** {internal_state}-2\n**Provides:** secret\n\nAudit.\n"
        ),
    );
    write_block_file(
        &block,
        "states.yaml",
        &format!(
            r#"name: {name}
version: 1
models: [internal-model]
states:
  {internal_state}:
    description: Review
    visits: 3
    agent: internal-agent
    model: internal-model
    mcp_servers: [internal-tracker]
    skills: [internal-skill]
    snapshot:
      emit:
        name: audit
        on: always
  internal-observer:
    description: Reuse the review snapshot
    agent: internal-agent
    model: internal-model
    snapshot:
      inherit:
        name: audit
        required: true
        select:
          state: {internal_state}
  internal-done:
    description: Done
    final: true
transitions:
  - {{ from: {internal_state}, to: internal-observer }}
  - {{ from: internal-observer, to: internal-done }}
profiles:
  primary: {{ initial: {internal_state}, allowed: [{internal_state}, internal-observer, internal-done] }}
node_policy:
  root: primary
  default: primary
"#,
        ),
    );
    write_block_file(
        &block,
        "settings.json",
        r#"{
  "agents": {
    "internal-agent": {
      "command": ["true"],
      "stdin_prompt": true,
      "session": {
        "resume": {"flag": "--continue"},
        "fork": {"flag": "--fork"},
        "session_dir_flag": "--session-dir",
        "layout": {"kind": "FlatById", "ext": "jsonl"}
      }
    }
  },
  "models": {
    "internal-model": {
      "provider": "fixture",
      "model": "fixture-model",
      "default_agent": "internal-agent"
    }
  },
  "mcp_servers": {
    "internal-tracker": {"command": ["true"]}
  },
  "skills": {
    "internal-skill": {"path": "skills/checklist"}
  }
}
"#,
    );
    write_block_file(&block, "skills/checklist/SKILL.md", "# Fixture checklist\n");
    block
}

pub(super) fn write_exposure_fixture(root: &Path) -> ExposureFixture {
    ExposureFixture { leaf: write_exposure_leaf(root, "review-block", "internal-ready") }
}

pub(super) fn write_observing_wrapper(
    root: &Path,
    name: &str,
    child: &Path,
    state_reference: &str,
) -> PathBuf {
    let wrapper = root.join(name);
    fs::create_dir_all(&wrapper).expect("create observing wrapper");
    write_block_file(
        &wrapper,
        "template.yaml",
        &format!(
            r#"name: {name}
version: 1
description: Consume a child's public identities
ports:
  entry: review.entry
  exits:
    done: review.done
use:
  - {{ block: {}, as: review }}
"#,
            child.display()
        ),
    );
    write_block_file(
        &wrapper,
        "index.rhei.md",
        &format!("# Rhei: Exposure observer\n**States:** {name}\n"),
    );
    write_block_file(
        &wrapper,
        "tasks/01-observe.md",
        &format!(
            "### Task observer: Observe\n**State:** {state_reference}-2\n**Prior:** Task review.audit\n\nObserve.\n"
        ),
    );
    write_block_file(
        &wrapper,
        "states.yaml",
        &format!(
            r#"name: {name}
version: 1
states:
  observed:
    description: Observe an exposed child
    agent: review.reviewer
    model: review.careful
    mcp_servers: [review.tracker]
    skills: [review.checklist]
    snapshot:
      inherit:
        name: audit
        required: false
        select:
          state: {state_reference}
  targeted:
    description: Use an exposed execution target
    target: review.reviewer:fixture:review.careful
  finished:
    description: Finished
    final: true
transitions:
  - {{ from: {state_reference}, to: observed }}
  - {{ from: observed, to: targeted }}
  - {{ from: targeted, to: finished }}
profiles:
  observer: {{ initial: {state_reference}, allowed: [{state_reference}, observed, targeted, finished] }}
node_policy:
  root: observer
  default: observer
"#,
        ),
    );
    wrapper
}

pub(super) fn instantiate_direct(dir: &Path, leaf: &Path, output: &Path) -> CliRun {
    let mount = mount_arg("review", leaf);
    run_compose(
        dir,
        &[
            "instantiate",
            "--mount",
            mount.as_str(),
            "--output",
            output.to_str().expect("output path"),
        ],
    )
}

pub(super) fn instantiate_curated(dir: &Path, wrapper: &Path, output: &Path) -> CliRun {
    run_compose(
        dir,
        &[
            "instantiate",
            wrapper.to_str().expect("wrapper path"),
            "--output",
            output.to_str().expect("output path"),
        ],
    )
}

pub(super) fn generated_text(root: &Path) -> String {
    super::block_composition_tests::text_files_below_for_runtime(root)
}
