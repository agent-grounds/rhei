//! Routing lookup follows the final flat policy, while declaration identity stays authored.
//! §FS-rhei-library.4.1
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use super::block_composition_support::*;
use super::block_provenance_source_tests::instantiate_one;
use super::block_provenance_support::*;
use super::*;

#[test]
fn routing_origins_follow_expansion_folding_and_final_duplicate_ordinals() {
    let dir = provenance_test_dir("routing");
    let block = dir.join("block");
    write_provenance_block(&block, false);
    fs::write(block.join("index.rhei.md"), "# Rhei: Routing\n**States:** provenance-block\n\n---\nstructure:\n  maxLevels: 4\n  nodeKinds: [task, panelist]\n---\n").unwrap();
    let path = block.join("states.yaml");
    let mut states = fs::read_to_string(&path).unwrap();
    states.push_str("  overrides:\n    - {match: {level: 2}, profile: primary}\n    - {match: {type: task, level: 2}, profile: primary}\n    - {match: {type: task, level: 3}, profile: primary}\n    - {match: {type: task, level: 3}, profile: primary}\n");
    fs::write(path, states).unwrap();
    let output = instantiate_one(&dir, block.to_str().unwrap(), "output");
    let lock = read_lock(&output);
    let machine = read_yaml(&output.join("states.yaml"));
    let rules = machine["node_policy"]["overrides"].as_sequence().unwrap();
    assert_eq!(rules.len(), 5, "level-only declaration fans out to two owned kinds");
    let mut duplicate = BTreeMap::<String, usize>::new();
    let mut expected = BTreeSet::from(["root".into(), "rhei".into(), "default".into()]);
    let mut routing_declarations = Vec::new();
    for rule in rules {
        assert_eq!(rule["profile"].as_str(), Some("flow"));
        let value = serde_json::to_value(rule).unwrap();
        let digest = canonical_digest(&value);
        let ordinal = duplicate.entry(digest.clone()).or_default();
        *ordinal += 1;
        let key = format!("overrides/{digest}~{ordinal}");
        expected.insert(key.clone());
        let origins = lock["nodes"]["routing"][&key].as_array().expect("final rule lookup");
        assert!(origins.iter().all(|origin| origin["via"] == "synthesized"));
        let declarations = origins
            .iter()
            .filter_map(|origin| {
                let id = origin["declaration"].as_str().unwrap();
                (lock["declarations"][id]["kind"] == "routing").then_some(id)
            })
            .collect::<Vec<_>>();
        assert_eq!(declarations.len(), 1, "each final rule retains its own authored declaration");
        routing_declarations.push(declarations[0]);
    }
    assert_eq!(object_keys(&lock["nodes"]["routing"]), expected, "no stale lookup keys");
    assert_eq!(routing_declarations[0], routing_declarations[1], "fan-out shares its declaration");
    assert_ne!(
        routing_declarations[1], routing_declarations[2],
        "final duplicates can have distinct authored origins"
    );
    assert!(duplicate.values().filter(|ordinal| **ordinal == 2).count() == 2);
    for id in &routing_declarations[..3] {
        assert!(
            lock["declarations"][id]["occurrence"].is_null(),
            "unique authored declarations use null"
        );
    }
    assert_eq!(lock["declarations"][routing_declarations[3]]["occurrence"], 1);
    assert_eq!(lock["declarations"][routing_declarations[4]]["occurrence"], 2);
    assert_complete_origins(&lock);
}

#[test]
fn by_type_primary_folding_retains_all_contributors_and_leaves_internal_routes_declared() {
    let dir = provenance_test_dir("by-type-routing");
    let a = dir.join("a");
    let b = dir.join("b");
    for block in [&a, &b] {
        write_provenance_block(block, false);
        fs::write(block.join("index.rhei.md"), "# Rhei: Routing\n**States:** provenance-block\n\n---\nstructure:\n  maxLevels: 4\n  nodeKinds: [task, panelist]\n---\n").unwrap();
        let path = block.join("states.yaml");
        let states = fs::read_to_string(&path).unwrap().replace(
            "node_policy:\n",
            "  internal: {initial: work, allowed: [work, done]}\nnode_policy:\n",
        );
        fs::write(path, format!("{states}  by_type: {{task: primary, panelist: internal}}\n"))
            .unwrap();
    }
    let output = dir.join("output");
    let result = rhei_command(dir.join(".home"))
        .current_dir(&dir)
        .env("GIT_CEILING_DIRECTORIES", fs::canonicalize(&dir).unwrap())
        .args([
            "instantiate",
            "--mount",
            &mount_arg("a", &a),
            "--mount",
            &mount_arg("b", &b),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_success(&CliRun::from(&result));
    let machine = read_yaml(&output.join("states.yaml"));
    let lock = read_lock(&output);
    let routes = &machine["node_policy"]["by_type"];
    let mounts = ["m1_a__", "m1_b__"];
    assert_ne!(lock["mounts"][mounts[0]]["source"], lock["mounts"][mounts[1]]["source"]);
    let declaration = |mount: &str, kind: &str, local: &str, rendered: serde_json::Value| {
        let source = &lock["mounts"][mount]["source"];
        let matches = lock["declarations"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, value)| {
                value["source"] == *source && value["kind"] == kind && value["local"] == local
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "one authored declaration for {mount}/{kind}/{local}");
        let (id, record) = matches[0];
        assert_eq!(record["file"], "states.yaml");
        assert_eq!(record["rendered"], canonical_digest(&rendered));
        assert!(record["occurrence"].is_null());
        id.clone()
    };
    let primary_origins = mounts.map(|mount| {
        let id = declaration(
            mount,
            "profile",
            "primary",
            serde_json::json!({"initial": "work", "allowed": ["work", "done"]}),
        );
        (mount.to_string(), id)
    });
    let mut expected_keys = BTreeSet::from(["root".into(), "rhei".into(), "default".into()]);
    assert_eq!(routes.as_mapping().unwrap().len(), 4);
    for mount in mounts {
        let qualified_task = format!("{mount}task");
        assert_eq!(routes[&qualified_task].as_str(), Some("flow"));
        let key = format!("by_type/{qualified_task}");
        expected_keys.insert(key.clone());
        let origins = lock["nodes"]["routing"][&key].as_array().expect("final folded route");
        let mut expected = BTreeSet::from(primary_origins.clone());
        expected.insert((
            mount.to_string(),
            declaration(mount, "routing", "by_type/task", serde_json::json!(["task", "primary"])),
        ));
        assert_eq!(origins.len(), expected.len(), "exactly one routing and both primary origins");
        let actual = origins
            .iter()
            .map(|origin| {
                assert_eq!(origin["via"], "synthesized");
                assert_eq!(origin["reason"], "folded-primary-routing");
                (
                    origin["mount"].as_str().unwrap().to_string(),
                    origin["declaration"].as_str().unwrap().to_string(),
                )
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected, "retain the exact contributors from both source mounts");

        let qualified_panelist = format!("{mount}panelist");
        assert_eq!(routes[&qualified_panelist].as_str().unwrap(), format!("{mount}internal"));
        let key = format!("by_type/{qualified_panelist}");
        expected_keys.insert(key.clone());
        let id = declaration(
            mount,
            "routing",
            "by_type/panelist",
            serde_json::json!(["panelist", "internal"]),
        );
        assert_eq!(
            lock["nodes"]["routing"][&key],
            serde_json::json!([{"mount": mount, "declaration": id, "via": "declared"}]),
            "an internal route keeps only its declared origin, with no synthesis reason"
        );
    }
    assert_eq!(object_keys(&lock["nodes"]["routing"]), expected_keys, "no stale routing keys");
    assert_complete_origins(&lock);
}
