//! What the run manifest names beyond the evidence records: each invocation's
//! accounting record, in a standalone workspace and in a Panta project, and
//! the published schemas of every document the evidence writes.
//! §FS-rhei-evidence.9.1 §FS-rhei-cost-accounting.8.1

use std::fs;
use std::path::{Path, PathBuf};

use super::accounting_unrecorded_spawn_support::{
    member_workspace, spawn_workspace, NOFAMILY_MACHINE, NOFAMILY_PLAN,
};
use super::evidence_support::{assert_store_exists, only_manifest, sha256_hex};
use super::{assert_success, repo_root, rhei_command, unique_temp_dir, CliRun};

/// Every accounting record under `root`, as `(path relative to root, sha256)`.
fn accounting_records(root: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.parent().is_some_and(|parent| parent.ends_with("accounting/invocations"))
                && path.extension().and_then(|ext| ext.to_str()) == Some("json")
            {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(root, &mut paths);
    let mut found: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let relative = path.strip_prefix(root).expect("under the root");
            let relative: Vec<String> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            (relative.join("/"), sha256_hex(&fs::read(path).expect("read record")))
        })
        .collect();
    found.sort();
    found
}

/// The manifest's `accounting` entries as `(path, sha256)`, sorted.
fn named_accounting(manifest: &serde_json::Value) -> Vec<(String, String)> {
    let entries = manifest["accounting"]
        .as_array()
        .unwrap_or_else(|| panic!("the manifest has an `accounting` list: {manifest:#}"));
    let mut named: Vec<(String, String)> = entries
        .iter()
        .map(|entry| {
            (
                entry["path"].as_str().expect("path").to_string(),
                entry["sha256"].as_str().expect("sha256").to_string(),
            )
        })
        .collect();
    named.sort();
    named
}

/// Two of four agent spawns have an accounting record; the manifest names
/// exactly those two, by path and hash. §FS-rhei-evidence.9.1
#[test]
fn the_manifest_names_each_accounting_record_by_path_and_hash() {
    let ws = spawn_workspace("evidence-accounting", NOFAMILY_PLAN, NOFAMILY_MACHINE);
    ws.run();
    let held = accounting_records(&ws.root);
    assert_eq!(held.len(), 2, "fixture: two priced spawns leave two records: {held:?}");

    let evidence = ws.root.join("runtime/evidence");
    assert_store_exists(&evidence);
    let (_, manifest) = only_manifest(&evidence);
    assert_eq!(named_accounting(&manifest), held);
}

/// In a Panta project the records live in the owning rhei's accounting root,
/// and the manifest's paths are relative to the project root, the run's root.
/// §FS-rhei-evidence.9.1 §FS-rhei-cost-accounting.2
#[test]
fn a_panta_projects_manifest_names_records_relative_to_the_project_root() {
    let ws = member_workspace("evidence-accounting-panta");
    ws.run();
    let held = accounting_records(&ws.root);
    assert!(!held.is_empty(), "fixture: the member's tickets leave accounting records");

    let evidence = ws.root.join("runtime/evidence");
    assert_store_exists(&evidence);
    let (_, manifest) = only_manifest(&evidence);
    assert_eq!(named_accounting(&manifest), held);
}

/// The ten published ids, sorted, as `rhei schema --list` prints them.
const PUBLISHED: [&str; 10] = [
    "rhei.accounting.cost.v1",
    "rhei.accounting.invocation.v1",
    "rhei.accounting.prices.v1",
    "rhei.accounting.summary.v1",
    "rhei.accounting.task.v1",
    "rhei.accounting.usage.v1",
    "rhei.evidence.event.v1",
    "rhei.evidence.invocation.v1",
    "rhei.evidence.manifest.v1",
    "rhei.summary.checkpoint.v1",
];

/// `rhei schema` lists and prints the evidence schemas and the summary
/// checkpoint's beside the accounting ones. §FS-rhei-cost-accounting.8.1
#[test]
fn rhei_schema_publishes_the_evidence_and_checkpoint_schemas() {
    let dir = unique_temp_dir("evidence-schema-list");
    let list = CliRun::from(
        &rhei_command(dir.join("home")).args(["schema", "--list"]).output().expect("schema runs"),
    );
    assert_success(&list);
    assert_eq!(list.stdout, PUBLISHED.join("\n") + "\n");

    for id in PUBLISHED.iter().filter(|id| !id.starts_with("rhei.accounting.")) {
        let printed = CliRun::from(
            &rhei_command(dir.join("home")).args(["schema", id]).output().expect("schema runs"),
        );
        assert_success(&printed);
        let source = fs::read_to_string(
            repo_root().join("crates/rhei-cli/schemas").join(format!("{id}.schema.json")),
        )
        .expect("versioned schema source");
        assert_eq!(printed.stdout, source, "{id} prints its versioned source");
    }
}
