//! `changeset-review` after the union: same name, same inputs, same ticket ids,
//! same artifact paths — and a machine whose states are the names their authors
//! wrote rather than `m6_review__split`.
//! §FS-rhei-library.1 §FS-rhei-library.6
//!
//! This is the one test that says the **non-breaking promise held**. Rule 1
//! refuses two `profiles.primary`, so `code-review` and `fix` are re-authored
//! with a profile and a node kind each, and `changeset-review` becomes the host
//! that writes the edge between them and the profile that spans them. What a
//! user could notice is only what a *new* instantiation's machine looks like;
//! everything a caller passes or reads is unchanged, and this pins that.
//!
//! The golden files under `fixtures/changeset-review-golden/` were captured
//! while the compiler was still live. The three surface tests here therefore
//! **pass today, by construction** — that is what a regression guard is, and it
//! is why they are not to be deleted for passing. The driven test below is the
//! half that cannot pass today.

use std::fs;
use std::path::PathBuf;

use super::into_support::run_into;
use super::*;

fn golden(name: &str) -> String {
    let path: PathBuf = repo_root().join("tests/e2e/fixtures/changeset-review-golden").join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// `--list-inputs` byte for byte. An input renamed, reordered, retyped or given
/// a different default is a break for every caller that scripts the template,
/// and re-authoring the internals must not be one.
/// §FS-rhei-library.3.4
///
/// Unix only, and the assertion is the stronger for it. `--list-inputs` quotes
/// a value for the shell it will be pasted into, which is the platform's own:
/// POSIX single quotes on Unix, `cmd`'s doubled double quotes on Windows
/// (§FS-rhei-errors.2). The golden was captured from the live block compiler,
/// on a POSIX host, and the compiler it was captured from is deleted by this
/// change — so there is no second capture to take, and no portable form of a
/// byte-for-byte golden exists. Comparing the two quoting styles modulo their
/// quotes would weaken what this guard claims on the two platforms where
/// byte-identity is simply true. The quoting rule keeps its three-platform
/// coverage elsewhere: `quote_for_posix` and `quote_for_cmd` are unit-tested
/// directly in `rhei-core`'s `platform.rs`, compiled everywhere.
/// §REQ-cross-platform.3
#[cfg(unix)]
#[test]
fn changeset_review_list_inputs_is_byte_identical() {
    let dir = unique_temp_dir("changeset-golden-inputs");

    let result = run_into(&["instantiate", "changeset-review", "--list-inputs"], &dir);
    assert_success(&result);
    assert_eq!(
        result.stdout,
        golden("list-inputs.txt"),
        "changeset-review's input surface must not move when its internals are re-authored"
    );
}

/// The ticket ids an instantiation writes. A ticket id is what a `**Prior:**`, a
/// `rhei next`, an `--task` and every artifact path's `{task_id}` resolve
/// through, so it is the part of the output callers actually hold.
#[test]
fn changeset_review_writes_the_same_ticket_ids() {
    let dir = unique_temp_dir("changeset-golden-ids");

    let result = run_into(
        &["instantiate", "changeset-review", "change_ref=GOLDEN", "--output", "out"],
        &dir,
    );
    assert_success(&result);

    let mut ids: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir.join("out/tasks")).expect("the workspace has task files") {
        let text = fs::read_to_string(entry.expect("dir entry").path()).expect("read task file");
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("### ") {
                if let Some((head, _)) = rest.split_once(':') {
                    ids.push(head.to_owned());
                }
            }
        }
    }
    ids.sort();
    let mut expected: Vec<String> = golden("ticket-ids.txt").lines().map(str::to_owned).collect();
    expected.sort();
    assert_eq!(ids, expected, "the ticket ids callers hold must not move");
}

/// Every artifact path the machine declares. `--into` leaves paths alone, so
/// re-authoring must too: a path is the contract between two states and, in this
/// template, between the review's `decide` and the fix's entry.
/// §FS-rhei-library.4
#[test]
fn changeset_review_declares_the_same_artifact_paths() {
    let dir = unique_temp_dir("changeset-golden-paths");

    let result = run_into(
        &["instantiate", "changeset-review", "change_ref=GOLDEN", "--output", "out"],
        &dir,
    );
    assert_success(&result);

    let machine = fs::read_to_string(dir.join("out/states.yaml")).expect("read machine");
    let mut paths: Vec<String> = machine
        .lines()
        .filter_map(|line| line.trim().strip_prefix("path: "))
        .map(str::to_owned)
        .collect();
    paths.sort();
    paths.dedup();
    let mut expected: Vec<String> =
        golden("artifact-paths.txt").lines().map(str::to_owned).collect();
    expected.sort();
    assert_eq!(paths, expected, "the artifact paths the states hand each other must not move");
}

/// The machine reads like a machine somebody wrote: `split` and `final-fix`
/// rather than `m6_review__split` and `m3_fix__final-fix`. This is the one thing
/// a user could notice, and the reason the change is worth making.
/// §FS-rhei-library.1
#[test]
fn changeset_review_states_keep_their_authors_names() {
    let dir = unique_temp_dir("changeset-names");

    let result = run_into(
        &["instantiate", "changeset-review", "change_ref=GOLDEN", "--output", "out"],
        &dir,
    );
    assert_success(&result);

    let machine = fs::read_to_string(dir.join("out/states.yaml")).expect("read machine");
    assert!(
        !machine.contains("m6_review__") && !machine.contains("m3_fix__"),
        "no alias-encoded state name survives the union; got:\n{machine}"
    );
    for state in ["  split:", "  review:", "  human-review:", "  final-fix:"] {
        assert!(machine.contains(state), "the union should carry {state}; got:\n{machine}");
    }
    // A prompt can therefore name a state, instead of being told to look one up.
    let coordinate = fs::read_to_string(dir.join("out/tasks/001-coordinate.md"))
        .expect("read the coordinate ticket");
    assert!(
        !coordinate.contains("compiled state names"),
        "the coordinate ticket no longer has to send its agent to states.yaml; got:\n{coordinate}"
    );
}

/// The same gate, in the **example** the repository ships and in the templates
/// that compose it. The example is a regenerated artifact of this change, so it
/// is the one place a reader can see the assembled machine, and the two
/// templates are where the gate and its exits are authored.
/// §FS-rhei-library.3.5 §FS-rhei-library.6
#[test]
fn changeset_review_human_review_state_is_gating_in_shipped_workflows() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    let example_path = repo_root.join("examples/changeset-review-example/states.yaml");
    // Through the directory-aware load `rhei validate` performs: a union copies
    // each part's `prompt_templates/*.md` beside `states.yaml` rather than
    // inlining them. §FS-rhei-library.2 §FS-rhei-states.4.4
    let machine = rhei_cli::rhei_validator::StateMachine::from_yaml_file(&example_path)
        .unwrap_or_else(|err| panic!("load {}: {err}", example_path.display()));
    let human_review = machine
        .states
        .get("human-review")
        .unwrap_or_else(|| panic!("{} missing human-review state", example_path.display()));
    assert!(human_review.gating, "{} should mark human-review as gating", example_path.display());
    assert!(
        machine
            .transitions
            .iter()
            .any(|rule| rule.from.0 == "decide" && rule.to.0 == "human-review"),
        "{} should route final decisions through human-review",
        example_path.display()
    );
    assert!(
        machine
            .transitions
            .iter()
            .any(|rule| rule.from.0 == "human-review" && rule.to.0 == "prepare-workspace"),
        "{} should require human approval before workspace preparation",
        example_path.display()
    );

    // The gate is declared by the template whose author wrote it, and the two
    // edges *out* of it by the host that chains the two parts — which is the
    // one thing no single template knows. §FS-rhei-library.3.5
    let review_path = repo_root.join("crates/rhei-cli/templates/code-review/states.yaml");
    let review = fs::read_to_string(&review_path).expect("read code-review states.yaml");
    let start = review
        .find("\n  human-review:\n")
        .unwrap_or_else(|| panic!("{} missing human-review block", review_path.display()));
    let end = review[start + 1..]
        .find("\n  completed:\n")
        .map(|offset| start + 1 + offset)
        .unwrap_or(review.len());
    let human_review_block = &review[start..end];
    assert!(
        human_review_block.contains("\n    gating: true\n"),
        "{} should mark human-review as gating",
        review_path.display()
    );
    assert!(
        review.contains("\n  - from: decide\n    to: human-review\n"),
        "{} should route final decisions through human-review",
        review_path.display()
    );

    let host_path = repo_root.join("crates/rhei-cli/templates/changeset-review/states.yaml");
    let host = fs::read_to_string(&host_path).expect("read changeset-review states.yaml");
    assert!(
        host.contains("\n  - from: human-review\n    to: prepare-workspace\n")
            && host.contains("\n  - from: human-review\n    to: final-fix\n"),
        "{} should require human approval before either fix path",
        host_path.display()
    );
}

/// The chain the wrapper writes actually runs: a driven ticket reaches the
/// review's gate and then the fix's entry. `code-review`'s `human-review` is
/// `final: true` with no outgoing edge today, so this cannot pass until its
/// author makes it a non-final gate — the union never un-finalizes a terminal.
/// §FS-rhei-library.3.1 §FS-rhei-library.3.5
#[test]
fn a_driven_changeset_review_reaches_human_review_and_then_final_fix() {
    let dir = unique_temp_dir("changeset-driven");

    let result = run_into(
        &["instantiate", "changeset-review", "change_ref=GOLDEN", "--output", "out"],
        &dir,
    );
    assert_success(&result);

    let machine = fs::read_to_string(dir.join("out/states.yaml")).expect("read machine");
    // The gate has a way out, which is what makes the chain a chain.
    let human_review =
        machine.split("  human-review:").nth(1).expect("the machine declares human-review");
    let body = human_review.split("\n  ").next().unwrap_or(human_review);
    assert!(
        !body.contains("final: true"),
        "a gate a chain is continued from cannot be terminal; got:\n{body}"
    );

    // Two edges out of the gate: approve with no fix, and continue into the fix.
    assert!(
        machine.contains("from: human-review"),
        "human-review needs outgoing edges; got:\n{machine}"
    );
    for target in ["to: completed", "to: final-fix"] {
        assert!(machine.contains(target), "the gate should reach {target}; got:\n{machine}");
    }

    // And the walk the runtime actually allows, move by move. `split`'s only
    // edge is to `completed`, because the states after it belong to the tickets
    // the coordinator appends — so the walk starts where the coordinator puts
    // it, with a ticket in `aggregate-reviews`.

    // Nothing is forced: each state's declared output is written before the
    // ticket may leave it. §FS-rhei-library.3.5
    let workspace = dir.join("out");
    write_fixture_file(
        &workspace.join("tasks"),
        "002-aggregate.md",
        "### Task aggregate: Aggregate the part reviews\n**State:** aggregate-reviews\n\nThe task the coordinator appends beside the part reviews.\n",
    );
    let loaded =
        rhei_cli::rhei_validator::StateMachine::from_yaml_file(workspace.join("states.yaml"))
            .expect("the instantiated machine loads");
    let chain = [
        ("aggregate-reviews", "validate-review"),
        ("validate-review", "propose-fixes"),
        ("propose-fixes", "aggregate-proposals"),
        ("aggregate-proposals", "decide"),
        ("decide", "human-review"),
        ("human-review", "final-fix"),
    ];
    for (from, to) in chain {
        satisfy_state_outputs(&loaded, &workspace, from, "out.aggregate");
        let moved = run_into(
            &[
                "transition",
                "out",
                "--task",
                "aggregate",
                "--from",
                from,
                "--to",
                to,
                "--no-callbacks",
            ],
            &dir,
        );
        assert!(
            moved.status.success(),
            "the chain must walk {from} -> {to}; got:\nstdout:\n{}\nstderr:\n{}",
            moved.stdout,
            moved.stderr
        );
    }
}

/// Write every artifact a state owes before a ticket may leave it, resolved for
/// one task. The machine's own declaration is the contract, so the walk
/// satisfies it rather than forcing past it. §FS-rhei-states.3.2
fn satisfy_state_outputs(
    machine: &rhei_cli::rhei_validator::StateMachine,
    workspace: &std::path::Path,
    state: &str,
    task_id: &str,
) {
    let def = machine.states.get(state).unwrap_or_else(|| panic!("the machine declares {state}"));
    let targets: Vec<String> = if def.all_targets.is_empty() {
        def.target.clone().into_iter().collect()
    } else {
        def.all_targets.clone()
    };
    for artifact in &def.outputs {
        let resolved = artifact.path.replace("{task_id}", task_id);
        let paths: Vec<String> = if resolved.contains("{target.slug}") {
            targets
                .iter()
                .map(|target| resolved.replace("{target.slug}", &slug_of(target)))
                .collect()
        } else {
            vec![resolved]
        };
        for path in paths {
            let at = workspace.join(path);
            fs::create_dir_all(at.parent().expect("an artifact path has a parent"))
                .expect("create the artifact directory");
            fs::write(&at, "fixture\n").expect("write the artifact the state owes");
        }
    }
}

fn slug_of(selector: &str) -> String {
    rhei_cli::rhei_validator::parse_execution_target(selector)
        .expect("a machine's target selector parses")
        .slug()
}
