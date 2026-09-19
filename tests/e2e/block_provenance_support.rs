//! Independent lock assertions for §FS-rhei-library.4.1.
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const LOCK_PATH: &str = ".agent-grounds/rhei/composition.lock.json";

pub(super) fn write_provenance_block(path: &Path, single: bool) {
    fs::create_dir_all(path).unwrap();
    let name = path.file_name().and_then(|name| name.to_str()).expect("fixture directory name");
    fs::write(path.join("template.yaml"), format!("name: {name}\nversion: 1\ndescription: Provenance fixture\nports:\n  entry: work\n  exits: {{done: done}}\n")).unwrap();
    fs::write(path.join("states.yaml"), "name: provenance-block\nversion: 1\nstates:\n  work: {description: Work}\n  done: {description: Done, final: true}\ntransitions:\n  - {from: work, to: done}\nprofiles:\n  primary: {initial: work, allowed: [work, done]}\nnode_policy:\n  root: primary\n  default: primary\n").unwrap();
    let index = "# Rhei: Provenance fixture\n**States:** provenance-block\n";
    let task = "### Task job: Work\n**State:** work\n\nDo the work.\n";
    if single {
        fs::write(path.join("plan.rhei.md"), format!("{index}\n## Tasks\n\n{task}")).unwrap();
    } else {
        fs::create_dir_all(path.join("tasks")).unwrap();
        fs::write(path.join("index.rhei.md"), index).unwrap();
        fs::write(path.join("tasks/work.md"), task).unwrap();
    }
}

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
    let lock = serde_json::from_str(&fs::read_to_string(path).expect("read composition lock"))
        .expect("composition lock should be valid JSON");
    assert_declaration_fingerprints(&lock);
    lock
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
                assert_eq!(mounts[mount]["source"], source, "origin source must match its mount");
            }
        }
    }
}

pub(super) fn task_ids(output: &Path) -> BTreeSet<String> {
    fn collect(tasks: &[rhei_core::ast::Task], ids: &mut BTreeSet<String>) {
        for task in tasks {
            ids.insert(task.id.to_string());
            collect(&task.children, ids);
        }
    }
    fn visit(path: &Path, structure: &rhei_core::ast::Structure, ids: &mut BTreeSet<String>) {
        for entry in fs::read_dir(path).expect("read generated task tree") {
            let path = entry.expect("task tree entry").path();
            if path.is_dir() {
                visit(&path, structure, ids);
            } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
                let text = fs::read_to_string(path).expect("read generated task");
                let tasks =
                    rhei_core::parser::parse_workspace_tasks_with_structure(&text, structure)
                        .expect("parse actual task declarations");
                collect(&tasks, ids);
            }
        }
    }
    let text = fs::read_to_string(output.join("index.rhei.md")).expect("read output index");
    let index = rhei_core::parser::parse_workspace_index(&text).expect("parse output index");
    let mut ids = BTreeSet::new();
    visit(&output.join("tasks"), &index.structure, &mut ids);
    ids
}

pub(super) fn canonical_digest(value: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    // Value maps use sorted keys; this oracle never calls the compiler's hash helper.
    format!("sha256:{:x}", Sha256::digest(serde_json::to_vec(value).expect("canonical JSON")))
}

pub(super) fn assert_declaration_fingerprints(lock: &serde_json::Value) {
    for (id, declaration) in lock["declarations"].as_object().expect("declarations") {
        let tuple = serde_json::json!([
            declaration["source"],
            declaration["kind"],
            declaration["file"],
            declaration["local"],
            declaration["rendered"],
            declaration["occurrence"]
        ]);
        assert_eq!(
            *id,
            format!("decl:{}:{}", declaration["kind"].as_str().unwrap(), canonical_digest(&tuple))
        );
    }
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
