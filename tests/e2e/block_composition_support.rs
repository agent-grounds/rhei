use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub(super) struct CompositionFixtures {
    pub review: PathBuf,
    pub fix: PathBuf,
    pub flow: PathBuf,
}

pub(super) fn run_compose(cwd: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn write_block_file(block: &Path, relative: &str, contents: &str) {
    let path = block.join(relative);
    fs::create_dir_all(path.parent().expect("block file parent"))
        .expect("create block file parent");
    write_fixture_file(block, relative, contents);
}

fn write_review_block(root: &Path) -> PathBuf {
    let block = root.join("review-block");
    fs::create_dir_all(&block).expect("create review block");
    write_block_file(
        &block,
        "template.yaml",
        r#"name: review-block
version: 1
description: Review one subject
inputs:
  - name: subject
    description: Subject to review
    default: default-subject
ports:
  entry: review
  exits:
    done: done
    cancelled: cancelled
data:
  outputs:
    report: { kind: state-file, state: review, name: report }
    findings: { kind: task-export, task: job, name: findings }
"#,
    );
    write_block_file(
        &block,
        "index.rhei.md",
        r#"# Rhei: Review {{subject}}
**States:** review-block

---
structure:
  maxLevels: 4
  nodeKinds: [task, panelist]
---
"#,
    );
    write_block_file(
        &block,
        "tasks/review.md",
        r#"### Task job: Review {{subject}}
**State:** review
**Provides:** findings

Review {{subject}}.

### Panelist panel: Independent panel for {{subject}}
**State:** panel
**Prior:** Task job
"#,
    );
    write_block_file(
        &block,
        "prompt_templates/shared.md",
        "Review the subject and preserve the local prompt reference.\n",
    );
    write_block_file(
        &block,
        "states.yaml",
        r#"name: review-block
version: 1
models: [alpha, beta]
states:
  review:
    description: Review the subject
    prompt_template: shared
    outputs:
      - { name: report, path: runtime/report.md }
  panel:
    description: Independent panel
    all_targets: ["mock:mock:alpha", "mock:mock:beta"]
  done:
    description: Review complete
    final: true
  cancelled:
    description: Review cancelled
    final: true
transitions:
  - { from: review, to: done }
  - { from: panel, to: done }
  - { from: "*", to: cancelled }
profiles:
  primary: { initial: review, allowed: [review, done, cancelled] }
  panel: { initial: panel, allowed: [panel, done, cancelled] }
node_policy:
  root: primary
  default: primary
  by_type: { panelist: panel }
"#,
    );
    write_block_file(
        &block,
        "settings.json",
        r#"{
  "agents": {"mock": {"command": ["true"], "stdin_prompt": true}},
  "models": {
    "alpha": {"provider": "mock", "model": "alpha", "default_agent": "mock"},
    "beta": {"provider": "mock", "model": "beta", "default_agent": "mock"}
  }
}
"#,
    );
    block
}

fn write_fix_block(root: &Path) -> PathBuf {
    let block = root.join("fix-block");
    fs::create_dir_all(&block).expect("create fix block");
    write_block_file(
        &block,
        "template.yaml",
        r#"name: fix-block
version: 1
description: Fix one subject
inputs:
  - name: subject
    description: Subject to fix
    default: default-subject
ports:
  entry: review
  exits:
    done: done
    cancelled: cancelled
data:
  inputs:
    report: { kind: state-file, state: review, name: report }
    findings: { kind: task-export, task: job, name: findings }
"#,
    );
    write_block_file(
        &block,
        "index.rhei.md",
        r#"# Rhei: Fix {{subject}}
**States:** fix-block
"#,
    );
    write_block_file(
        &block,
        "tasks/fix.md",
        r#"### Task job: Fix {{subject}}
**State:** review

Fix {{subject}}.
"#,
    );
    write_block_file(
        &block,
        "prompt_templates/shared.md",
        "Fix the subject and preserve the local prompt reference.\n",
    );
    write_block_file(
        &block,
        "states.yaml",
        r#"name: fix-block
version: 1
states:
  review:
    description: Apply the fix
    prompt_template: shared
    inputs:
      - { name: report, path: runtime/report.md }
  done:
    description: Fix complete
    final: true
  cancelled:
    description: Fix cancelled
    final: true
transitions:
  - { from: review, to: done }
  - { from: "*", to: cancelled }
profiles:
  primary: { initial: review, allowed: [review, done, cancelled] }
node_policy:
  root: primary
  default: primary
"#,
    );
    block
}

fn write_curated_flow(root: &Path) -> PathBuf {
    let block = root.join("curated-flow");
    fs::create_dir_all(&block).expect("create curated block");
    write_block_file(
        &block,
        "template.yaml",
        r#"name: curated-flow
version: 1
description: Review and then fix
inputs:
  - name: subject
    description: Subject to process
    positional: 1
ports:
  entry: review.entry
  exits:
    done: fix.done
use:
  - { block: ../review-block, as: review }
  - { block: ../fix-block, as: fix }
bind:
  - { input: subject, to: review.subject }
  - { input: subject, to: fix.subject }
seams:
  - from: review.done
    to: fix.entry
    pass:
      review.report: fix.report
      review.findings: fix.findings
"#,
    );
    block
}

pub(super) fn write_composition_fixtures(root: &Path) -> CompositionFixtures {
    let review = write_review_block(root);
    let fix = write_fix_block(root);
    let flow = write_curated_flow(root);
    CompositionFixtures { review, fix, flow }
}

/// The compiler names a block by its resolved manifest path, so a diagnostic
/// on Windows prints the canonical `\\?\` spelling of a temp directory the
/// test created under its 8.3 alias; compare against the same resolution.
pub(super) fn resolved_path(path: &Path) -> String {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

pub(super) fn mount_arg(alias: &str, path: &Path) -> String {
    format!("{alias}={}", path.display())
}

pub(super) fn read_yaml(path: &Path) -> serde_yaml::Value {
    serde_yaml::from_str(&fs::read_to_string(path).expect("read YAML")).expect("valid YAML")
}

pub(super) fn assert_failed_with(result: &CliRun, fragments: &[&str]) {
    assert!(
        !result.status.success(),
        "command should fail\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    for fragment in fragments {
        assert!(
            combined.contains(fragment),
            "failure should contain {fragment:?}\nstdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
    }
}
