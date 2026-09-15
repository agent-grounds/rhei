//! Fixtures and assertions for the effective roster inspection contract.
//! §FS-rhei-agents.1.1.7

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;

pub const CURRENT_SETTINGS: &str = ".agent-grounds/rhei/settings.json";
pub const DEPRECATED_SETTINGS: &str = ".agents/rhei/settings.json";

pub fn write_plan(root: &Path, name: &str, body: &str) -> PathBuf {
    write_fixture_file(root, name, body)
}

pub fn write_panta(root: &Path) -> PathBuf {
    write_fixture_file(root, "index.panta.md", "# Panta: Roster fixture\n")
}

pub fn write_settings(root: &Path, relative: &str, contents: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("settings path has a parent"))
        .expect("create settings directory");
    fs::write(&path, contents).expect("write settings fixture");
    path
}

pub fn run_roster(home: &Path, cwd: &Path, target: Option<&Path>, json: bool) -> CliRun {
    let mut command = rhei_command(home);
    command.current_dir(cwd).arg("roster");
    if let Some(target) = target {
        command.arg(target);
    }
    if json {
        command.arg("--json");
    }
    CliRun::from(&command.output().expect("rhei roster should execute"))
}

pub fn run_roster_args(home: &Path, cwd: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(home)
        .current_dir(cwd)
        .arg("roster")
        .args(args)
        .output()
        .expect("rhei roster should execute");
    CliRun::from(&output)
}

pub fn roster_json(result: &CliRun) -> Value {
    assert_success(result);
    serde_json::from_str(&result.stdout).unwrap_or_else(|error| {
        panic!(
            "rhei roster --json must emit one JSON object: {error}\nstdout:\n{}\nstderr:\n{}",
            result.stdout, result.stderr
        )
    })
}

pub fn resolved(path: &Path) -> String {
    fs::canonicalize(path).expect("fixture path should resolve").display().to_string()
}

/// A stable snapshot of paths, entry kinds, and file bytes under a fixture.
pub fn tree_snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn walk(root: &Path, dir: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        let mut children = fs::read_dir(dir)
            .expect("read fixture tree")
            .map(|entry| entry.expect("read fixture entry"))
            .collect::<Vec<_>>();
        children.sort_by_key(|entry| entry.file_name());
        for entry in children {
            let path = entry.path();
            let relative = path.strip_prefix(root).expect("entry belongs to fixture").to_path_buf();
            if entry.file_type().expect("read fixture type").is_dir() {
                entries.insert(relative, None);
                walk(root, &path, entries);
            } else {
                entries.insert(relative, Some(fs::read(&path).expect("read fixture file")));
            }
        }
    }

    let mut entries = BTreeMap::new();
    walk(root, root, &mut entries);
    entries
}
