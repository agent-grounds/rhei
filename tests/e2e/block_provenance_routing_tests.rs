//! Override lookup follows the final flat policy, while declaration identity stays authored.
//! §FS-rhei-library.4.1
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use super::block_composition_support::*;
use super::block_provenance_source_tests::instantiate_one;
use super::block_provenance_support::*;

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
