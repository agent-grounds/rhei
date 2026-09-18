use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const LOCK_PATH: &str = ".agent-grounds/rhei/composition.lock.json";

pub(super) fn provenance_test_dir(prefix: &str) -> super::TestDir {
    let root = std::env::var_os("RHEI_TEST_SCRATCH_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    super::TestDir::create(root.join(format!("rhei-issue-284-{prefix}-{nanos}")))
}

pub(super) fn read_lock(output: &Path) -> serde_json::Value {
    let path = output.join(LOCK_PATH);
    assert!(
        path.is_file(),
        "composed workspace should contain the v1 composition lock at {}",
        path.display()
    );
    serde_json::from_str(&fs::read_to_string(path).expect("read composition lock"))
        .expect("composition lock should be valid JSON")
}

pub(super) fn object_keys(value: &serde_json::Value) -> BTreeSet<String> {
    value.as_object().expect("JSON object").keys().cloned().collect()
}

pub(super) fn yaml_keys(value: &serde_yaml::Value) -> BTreeSet<String> {
    value
        .as_mapping()
        .expect("YAML mapping")
        .keys()
        .map(|key| key.as_str().expect("string YAML key").to_string())
        .collect()
}

pub(super) fn assert_complete_origins(lock: &serde_json::Value) {
    let sources = lock["sources"].as_object().expect("sources table");
    let mounts = lock["mounts"].as_object().expect("mounts table");
    let declarations = lock["declarations"].as_object().expect("declarations table");
    for kind in ["states", "tasks", "profiles", "routing"] {
        for (node, origins) in lock["nodes"][kind].as_object().expect("node table") {
            let origins = origins
                .as_array()
                .unwrap_or_else(|| panic!("{kind}/{node} origins should be an array"));
            assert!(!origins.is_empty(), "{kind}/{node} should retain an origin");
            let encoded = origins
                .iter()
                .map(|origin| {
                    format!(
                        "{}\0{}\0{}\0{}",
                        origin["mount"].as_str().expect("origin mount"),
                        origin["declaration"].as_str().expect("origin declaration"),
                        origin["via"].as_str().expect("origin via"),
                        origin["reason"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>();
            let mut sorted = encoded.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(encoded, sorted, "{kind}/{node} origins should be canonical");
            for origin in origins {
                let mount = origin["mount"].as_str().expect("origin mount");
                let declaration = origin["declaration"].as_str().expect("origin declaration");
                assert!(mounts.contains_key(mount), "missing mount {mount} for {kind}/{node}");
                let source = declarations[declaration]["source"]
                    .as_str()
                    .unwrap_or_else(|| panic!("missing declaration {declaration}"));
                assert!(sources.contains_key(source), "missing source {source} for {kind}/{node}");
            }
        }
    }
}

pub(super) fn task_ids(output: &Path) -> BTreeSet<String> {
    fn visit(path: &Path, ids: &mut BTreeSet<String>) {
        for entry in fs::read_dir(path).expect("read generated task tree") {
            let path = entry.expect("task tree entry").path();
            if path.is_dir() {
                visit(&path, ids);
            } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
                for line in fs::read_to_string(path).expect("read generated task").lines() {
                    let heading = line.trim_start_matches('#').trim();
                    let Some((identity, _)) = heading.split_once(':') else { continue };
                    if let Some(id) = identity.split_whitespace().last() {
                        ids.insert(id.to_string());
                    }
                }
            }
        }
    }
    let mut ids = BTreeSet::new();
    visit(&output.join("tasks"), &mut ids);
    ids
}

pub(super) fn assert_sorted_json_objects(value: &serde_json::Value, source: &str) {
    match value {
        serde_json::Value::Object(map) => {
            let keys = map.keys().collect::<Vec<_>>();
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(keys, sorted, "object keys are not canonical in:\n{source}");
            for child in map.values() {
                assert_sorted_json_objects(child, source);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                assert_sorted_json_objects(child, source);
            }
        }
        _ => {}
    }
}
