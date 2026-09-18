//! Durable provenance is emitted beside, but does not replace, the flat runtime workspace.
//! §FS-rhei-library.4.1 §FS-rhei-library.7.1 §FS-rhei-templates.6.1.2
use std::collections::BTreeSet;
use std::fs;

use super::block_composition_support::*;
use super::block_provenance_support::*;
use super::*;

fn write_root_wrapper(dir: &std::path::Path) -> std::path::PathBuf {
    let wrapper = dir.join("root-wrapper");
    fs::create_dir_all(wrapper.join("tasks")).expect("root wrapper directories");
    write_fixture_file(
        &wrapper,
        "template.yaml",
        r#"name: root-wrapper
version: 1
description: Root declarations around a nested compatibility wrapper
inputs:
  - name: change_ref
    description: Change to review
    default: HEAD~3
ports:
  entry: inner.entry
  exits:
    done: inner.done
    cancelled: inner.cancelled
use:
  - { block: changeset-review, as: inner }
bind:
  - { input: change_ref, to: inner.change_ref }
"#,
    );
    write_fixture_file(
        &wrapper,
        "index.rhei.md",
        "# Rhei: Root wrapper\n**States:** root-wrapper\n",
    );
    write_fixture_file(
        &wrapper,
        "tasks/root.md",
        "### Task root: Root-owned task\n**State:** root-note\n\nRecord the wrapper result.\n",
    );
    write_fixture_file(
        &wrapper,
        "states.yaml",
        r#"name: root-wrapper
version: 1
states:
  root-note: { description: Record wrapper result }
  root-done: { description: Wrapper result recorded, final: true }
transitions:
  - { from: root-note, to: root-done }
profiles:
  root-lane: { initial: root-note, allowed: [root-note, root-done] }
node_policy: { root: root-lane, default: root-lane }
"#,
    );
    wrapper
}

#[test]
fn composition_lock_covers_flat_nodes_nested_mounts_renames_and_coalescing() {
    let dir = provenance_test_dir("complete");
    let wrapper = write_root_wrapper(&dir);
    let output = dir.join("composed");
    let result = run_compose(
        &dir,
        &[
            "instantiate",
            wrapper.to_str().expect("wrapper path"),
            "--set",
            "change_ref=HEAD~3",
            "--output",
            output.to_str().expect("output path"),
        ],
    );
    assert_success(&result);

    let lock = read_lock(&output);
    assert_eq!(lock["schema_version"], 1);
    assert_eq!(
        object_keys(&lock),
        BTreeSet::from([
            "declarations".into(),
            "mounts".into(),
            "nodes".into(),
            "schema_version".into(),
            "sources".into(),
        ])
    );
    assert_eq!(lock["mounts"]["root"]["chain"], serde_json::json!([]));
    for chain in [
        serde_json::json!(["inner"]),
        serde_json::json!(["inner", "review"]),
        serde_json::json!(["inner", "fix"]),
    ] {
        assert!(
            lock["mounts"]
                .as_object()
                .expect("mounts")
                .values()
                .any(|mount| mount["chain"] == chain),
            "missing nested mount chain {chain}"
        );
    }

    let machine = read_yaml(&output.join("states.yaml"));
    assert_eq!(object_keys(&lock["nodes"]["states"]), yaml_keys(&machine["states"]));
    assert_eq!(object_keys(&lock["nodes"]["profiles"]), yaml_keys(&machine["profiles"]));
    assert_eq!(object_keys(&lock["nodes"]["tasks"]), task_ids(&output));
    for route in ["root", "rhei", "default"] {
        assert!(lock["nodes"]["routing"].get(route).is_some(), "missing routing key {route}");
    }
    for (kind, node) in [("states", "root-note"), ("tasks", "root")] {
        assert!(lock["nodes"][kind][node]
            .as_array()
            .expect("root origins")
            .iter()
            .any(|origin| origin["mount"] == "root"));
    }

    let completed = lock["nodes"]["states"]["m5_inner__completed"]
        .as_array()
        .expect("coalesced completed origins");
    assert_eq!(completed.len(), 2, "coalescing should retain both terminal declarations");
    assert_ne!(completed[0]["declaration"], completed[1]["declaration"]);
    assert_eq!(
        lock["declarations"]
            .as_object()
            .unwrap()
            .values()
            .map(|declaration| declaration["kind"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["state", "task", "profile", "routing"])
    );
    let coordinate = &lock["nodes"]["tasks"]["m5_inner__coordinate"];
    assert!(
        coordinate.as_array().expect("coordinate origins").iter().any(|origin| {
            origin["via"] == "declared"
                && lock["mounts"][origin["mount"].as_str().expect("mount")]["chain"]
                    == serde_json::json!(["inner", "review"])
        }),
        "compatibility rename should retain the review declaration"
    );
    assert!(
        lock["nodes"]["profiles"].as_object().expect("profiles").values().any(|origins| {
            origins
                .as_array()
                .expect("profile origins")
                .iter()
                .any(|origin| origin["via"] == "synthesized" && origin["reason"].is_string())
        }),
        "derived flow profile should name its synthesis reason"
    );
    assert!(
        lock["nodes"]["routing"].as_object().expect("routing").values().any(|origins| {
            let origins = origins.as_array().expect("routing origins");
            origins.len() > 1
                && origins
                    .iter()
                    .all(|origin| origin["via"] == "synthesized" && origin["reason"].is_string())
        }),
        "synthesized routing should retain all contributors and its reason"
    );
    assert_complete_origins(&lock);
    assert_declaration_fingerprints(&lock);

    let headers = fs::read_to_string(output.join("states.yaml")).expect("states header");
    assert!(headers.contains(LOCK_PATH) && headers.contains("src:sha256:"));
    assert!(!headers.contains(dir.to_str().expect("fixture path")), "header leaked a host path");

    assert_success(&run_compose(&dir, &["validate", output.to_str().expect("output path")]));
    assert_success(&run_compose(
        &dir,
        &["next", output.to_str().expect("output path"), "--no-callbacks", "--json"],
    ));
}

#[test]
fn composition_lock_distinguishes_repeated_mounts_and_is_canonical() {
    let dir = provenance_test_dir("repeat");
    let fixtures = write_composition_fixtures(&dir);
    let states_path = fixtures.review.join("states.yaml");
    let states = fs::read_to_string(&states_path).expect("review states").replace(
        "  by_type: { panelist: panel }",
        "  by_type: { panelist: panel }\n  overrides:\n    - { match: { type: panelist }, profile: panel }\n    - { match: { type: panelist }, profile: panel }",
    );
    fs::write(states_path, states).expect("add duplicate routing declarations");

    let first_mount = mount_arg("first", &fixtures.review);
    let second_mount = mount_arg("second", &fixtures.review);
    let mut outputs = Vec::new();
    for name in ["one", "two"] {
        let output = dir.join(name);
        assert_success(&run_compose(
            &dir,
            &[
                "instantiate",
                "--mount",
                &first_mount,
                "--mount",
                &second_mount,
                "--output",
                output.to_str().expect("output path"),
            ],
        ));
        outputs.push(output);
    }

    let first_text = fs::read_to_string(outputs[0].join(LOCK_PATH)).unwrap_or_else(|_| {
        panic!("composed workspace should contain the v1 composition lock at {LOCK_PATH}")
    });
    let second_text = fs::read_to_string(outputs[1].join(LOCK_PATH)).expect("second lock");
    assert_eq!(first_text, second_text, "same inputs and bytes should produce the same lock");
    assert!(first_text.ends_with('\n') && !first_text.ends_with("\n\n"));
    let lock: serde_json::Value = serde_json::from_str(&first_text).expect("lock JSON");
    let canonical = format!("{}\n", serde_json::to_string_pretty(&lock).expect("canonical JSON"));
    assert_eq!(first_text, canonical, "lock should use canonical pretty JSON");
    assert_sorted_json_objects(&lock, &first_text);

    let first_source = lock["mounts"]["m5_first__"]["source"].as_str().expect("first source");
    let second_source = lock["mounts"]["m6_second__"]["source"].as_str().expect("second source");
    assert_eq!(first_source, second_source, "repeated mounts should share source identity");
    assert_eq!(lock["sources"][first_source]["locators"].as_array().unwrap().len(), 1);
    assert_eq!(
        lock["nodes"]["states"]["m5_first__review"][0]["declaration"],
        lock["nodes"]["states"]["m6_second__review"][0]["declaration"]
    );
    for state in ["m5_first__review", "m6_second__review"] {
        assert!(lock["nodes"]["states"].get(state).is_some(), "missing repeated state {state}");
    }
    let override_keys = object_keys(&lock["nodes"]["routing"])
        .into_iter()
        .filter(|key| key.starts_with("overrides/"))
        .collect::<Vec<_>>();
    assert!(override_keys.iter().any(|key| key.ends_with("~1")));
    assert!(override_keys.iter().any(|key| key.ends_with("~2")));
    for (id, declaration) in lock["declarations"].as_object().expect("declarations") {
        assert!(id.starts_with("decl:") && id.contains(":sha256:"));
        assert!(declaration["rendered"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")));
    }
    assert_complete_origins(&lock);
    assert_declaration_fingerprints(&lock);
}

#[test]
fn dry_run_reports_and_validates_the_lock_without_writing_it() {
    let dir = provenance_test_dir("dry-run");
    let fixtures = write_composition_fixtures(&dir);
    let output = dir.join("dry-output");
    let mount = mount_arg("review", &fixtures.review);
    let dry = run_compose(
        &dir,
        &[
            "instantiate",
            "--mount",
            &mount,
            "--dry-run",
            "--output",
            output.to_str().expect("output path"),
        ],
    );
    assert_success(&dry);
    let tree = dry.stdout.lines().collect::<Vec<_>>();
    assert!(
        tree.windows(3).any(|lines| {
            lines[0] == "  |-- .agent-grounds/"
                && lines[1] == "  |   `-- rhei/"
                && matches!(
                    lines[2],
                    "  |       |-- composition.lock.json" | "  |       `-- composition.lock.json"
                )
        }),
        "dry-run tree should list the lock under .agent-grounds/rhei:\n{}",
        dry.stdout
    );
    assert!(!output.exists(), "dry run wrote its output");

    let legacy = dir.join("legacy");
    assert_success(&run_compose(
        &dir,
        &[
            "instantiate",
            fixtures.review.to_str().expect("review path"),
            "--output",
            legacy.to_str().expect("legacy path"),
        ],
    ));
    assert!(!legacy.join(LOCK_PATH).exists(), "legacy single-template behavior changed");
}
