//! A target collection written in flow style is refused before the union
//! writes or validates anything, naming the target's own file and the key.
//! §FS-rhei-library.7.4
//!
//! The refusals fail today because the union splices block-style lines into
//! the braces and then refuses its own output as a YAML syntax error in a
//! scratch copy: `failed to parse state machine '/tmp/.tmpXXXX/release/states.yaml'`.

use std::path::Path;

use super::into_support::*;
use super::*;

/// A host machine with one block replaced by its flow-style spelling.
fn flow_host(prefix: &str, block: &str, flow: &str) -> (TestDir, std::path::PathBuf) {
    let (dir, root) = host_workspace(prefix);
    assert!(HOST_MACHINE.contains(block), "the fixture block to replace is in the host machine");
    write_fixture_file(&root, "states.yaml", &HOST_MACHINE.replace(block, flow));
    write_review_template(&dir);
    (dir, root)
}

/// Place `review-loop` into the host and assert the named refusal: non-zero,
/// the target's own `states.yaml` and the dotted key in the message, the block
/// style remedy, no scratch path, and the target's bytes unchanged.
fn assert_flow_refusal(dir: &Path, root: &Path, key: &str, extra: &[&str]) {
    let before = read(&root.join("states.yaml"));
    let mut args = vec!["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"];
    args.extend_from_slice(extra);
    let result = run_into(&args, dir);

    assert!(
        !result.status.success(),
        "a flow-style `{key}` the union must extend is refused\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        !result.stderr.contains("failed to parse state machine"),
        "the union's own output is never reported as the user's invalid YAML; got:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains(".tmp"),
        "the message names the target's file, never the scratch copy; got:\n{}",
        result.stderr
    );
    let host_file = Path::new("release").join("states.yaml");
    assert_stderr_contains(&result, &host_file.display().to_string());
    assert_stderr_contains(&result, key);
    assert_stderr_contains(&result, "block style");
    assert_eq!(before, read(&root.join("states.yaml")), "the target is byte-identical");
}

/// Every line of `before` is in `after`, in order: the union only added lines.
fn keeps_every_line(before: &str, after: &str) -> bool {
    let mut rest = after.lines();
    before.lines().all(|line| rest.any(|candidate| candidate == line))
}

const BY_TYPE_BLOCK: &str = "  by_type:\n    task: host\n";
const PROFILES_BLOCK: &str =
    "profiles:\n  host:\n    initial: pending\n    allowed: [pending, completed, cancelled]\n";
const TRANSITIONS_BLOCK: &str =
    "transitions:\n  - from: pending\n    to: completed\n  - from: \"*\"\n    to: cancelled\n";
const STATES_BLOCK: &str = "states:\n  pending:\n    description: Ready for work\n    instructions: |\n      Do the work and move on.\n  completed:\n    final: true\n    description: Done\n  cancelled:\n    final: true\n    description: Abandoned\n";

/// The report's shape: `by_type: { task: host }` gains the template's `step`
/// route only by rewriting its line. §FS-rhei-library.7.4
#[test]
fn a_flow_by_type_the_union_extends_is_refused_naming_the_key() {
    let (dir, root) = flow_host("into-flow-by-type", BY_TYPE_BLOCK, "  by_type: { task: host }\n");
    assert_flow_refusal(&dir, &root, "node_policy.by_type", &[]);
}

/// `--dry-run` refuses the same way rather than printing a diff the write
/// could never produce. §FS-rhei-library.7.4
#[test]
fn a_dry_run_refuses_a_flow_by_type_the_same_way() {
    let (dir, root) =
        flow_host("into-flow-by-type-dry", BY_TYPE_BLOCK, "  by_type: { task: host }\n");
    assert_flow_refusal(&dir, &root, "node_policy.by_type", &["--dry-run"]);
}

/// A top-level flow collection is refused too: the template brings a profile
/// of its own. §FS-rhei-library.7.4
#[test]
fn a_flow_profiles_block_the_union_extends_is_refused() {
    let (dir, root) = flow_host(
        "into-flow-profiles",
        PROFILES_BLOCK,
        "profiles: { host: { initial: pending, allowed: [pending, completed, cancelled] } }\n",
    );
    assert_flow_refusal(&dir, &root, "profiles", &[]);
}

/// A flow sequence: the template brings transitions of its own.
/// §FS-rhei-library.7.4
#[test]
fn a_flow_transitions_block_the_union_extends_is_refused() {
    let (dir, root) = flow_host(
        "into-flow-transitions",
        TRANSITIONS_BLOCK,
        "transitions: [ { from: pending, to: completed }, { from: \"*\", to: cancelled } ]\n",
    );
    assert_flow_refusal(&dir, &root, "transitions", &[]);
}

/// The template brings states of its own. §FS-rhei-library.7.4
#[test]
fn a_flow_states_block_the_union_extends_is_refused() {
    let (dir, root) = flow_host(
        "into-flow-states",
        STATES_BLOCK,
        "states: { pending: { description: Ready for work, instructions: Do the work and move on. }, \
         completed: { final: true, description: Done }, cancelled: { final: true, description: Abandoned } }\n",
    );
    assert_flow_refusal(&dir, &root, "states", &[]);
}

/// Flow-style *entries* inside block collections are not what the refusal is
/// about: the union inserts beside them, so it succeeds and the result
/// validates. §FS-rhei-library.7.4
#[test]
fn flow_entries_inside_block_collections_still_union() {
    let machine = r#"name: host
version: 1
states:
  pending: { description: Ready for work, instructions: Do the work and move on. }
  completed: { final: true, description: Done }
  cancelled: { final: true, description: Abandoned }
transitions:
  - { from: pending, to: completed }
  - { from: "*", to: cancelled }
profiles:
  host: { initial: pending, allowed: [pending, completed, cancelled] }
node_policy:
  root: host
  default: host
  by_type:
    task: host
"#;
    let (dir, root) = host_workspace("into-flow-entries");
    write_fixture_file(&root, "states.yaml", machine);
    write_review_template(&dir);

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&result);
    let after = read(&root.join("states.yaml"));
    assert!(
        keeps_every_line(machine, &after),
        "the host's lines are kept as written; got:\n{after}"
    );
    assert!(after.contains("    step: review-loop\n"), "the route is added; got:\n{after}");
    assert_success(&run_into(&["validate", "release"], &dir));
}

/// A flow collection the union adds nothing to is left alone and not refused:
/// the template's only route is already in the host's flow `by_type`.
/// §FS-rhei-library.7.4
#[test]
fn a_flow_by_type_the_union_adds_nothing_to_is_not_refused() {
    let (dir, root) = flow_host(
        "into-flow-by-type-unchanged",
        BY_TYPE_BLOCK,
        "  by_type: { task: host, step: review-loop }\n",
    );

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&result);
    let after = read(&root.join("states.yaml"));
    assert!(
        after.contains("  by_type: { task: host, step: review-loop }\n"),
        "the flow line is kept as written; got:\n{after}"
    );
    assert_success(&run_into(&["validate", "release"], &dir));
}
