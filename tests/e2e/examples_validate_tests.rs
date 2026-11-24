//! Every example `cargo xtask examples validate` lists validates against the
//! machine its entry names, and one that names none against the built-in `rhei`
//! machine. Nothing else in CI runs that gate, which is how
//! `examples/human-review-loop.rhei.md` stayed on states the built-in machine
//! dropped for six months (agent-grounds/rhei#766). §FS-rhei-authoring.6
//!
//! The table is read from `xtask/src/main.rs` rather than copied here, so an
//! example added there is held to this test with no second list to forget.

use std::fs;

use super::*;

/// One `EXAMPLES` entry: its name, its path, and the machine it names.
struct Entry {
    name: String,
    path: String,
    state_machine: Option<String>,
}

/// The quoted value of `field: "…"` on `line`, if the line sets that field.
fn quoted(line: &str, field: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(field)?.strip_prefix(':')?.trim();
    let rest = rest.strip_prefix("Some(").unwrap_or(rest);
    let value = rest.strip_prefix('"')?;
    Some(value[..value.find('"')?].to_string())
}

/// The entries of xtask's `EXAMPLES` table, in the order it lists them.
fn xtask_examples() -> Vec<Entry> {
    let source = fs::read_to_string(repo_root().join("xtask/src/main.rs")).expect("xtask source");
    let table = source
        .split_once("const EXAMPLES: &[Example] = &[")
        .and_then(|(_, rest)| rest.split_once("\n];"))
        .map(|(table, _)| table)
        .expect("xtask declares its EXAMPLES table");
    let mut entries = Vec::new();
    for block in table.split("Example {").skip(1) {
        let mut name = None;
        let mut path = None;
        let mut state_machine = None;
        for line in block.lines() {
            name = name.or_else(|| quoted(line, "name"));
            path = path.or_else(|| quoted(line, "path"));
            state_machine = state_machine.or_else(|| quoted(line, "state_machine"));
        }
        entries.push(Entry {
            name: name.expect("every entry has a name"),
            path: path.expect("every entry has a path"),
            state_machine,
        });
    }
    entries
}

#[test]
#[ignore = "red until #766: human-review-loop example uses states the built-in machine dropped; implement removes this"]
fn every_xtask_example_validates_against_the_machine_it_names() {
    let root = repo_root();
    let entries = xtask_examples();
    assert!(
        entries.iter().any(|e| e.name == "human-review-loop" && e.state_machine.is_none()),
        "the table is read, and human-review-loop is judged by the built-in machine"
    );
    assert!(entries.len() >= 16, "every hand-written example is read, found {}", entries.len());

    let home = unique_scratchpad_dir("examples-validate");
    let mut failures = Vec::new();
    for entry in &entries {
        // A machine left in the environment by an outer run, or the
        // developer's own config, would judge the example by something else.
        let mut cmd = rhei_command(&home);
        cmd.current_dir(&root).env_remove("XDG_CONFIG_HOME").env_remove("XDG_DATA_HOME");
        if let Some(machine) = &entry.state_machine {
            cmd.args(["--state-machine", machine]);
        }
        let out = cmd.args(["validate", &entry.path]).output().expect("spawn rhei validate");
        if !out.status.success() {
            failures.push(format!("{} ({}):\n{}", entry.name, entry.path, stderr(&out)));
        }
    }
    assert!(failures.is_empty(), "examples that do not validate:\n{}", failures.join("\n"));
}
