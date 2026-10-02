//! Fixtures for the artifact-path diagnostics of a union: machines whose states
//! declare one rhei-scoped path as writers or as readers, laid out as a
//! `--into` target, as a placed template, or as the parts of an `includes:`.
//! §FS-rhei-library.7.2
//!
//! The shape is the point of every test that uses these, so a fixture is
//! written as a list of states and what each declares rather than as YAML: the
//! table a reader checks the test against is the call itself.

use std::path::{Path, PathBuf};

use super::into_support::host_workspace;
use super::*;

/// The path every shared-artifact fixture declares unless a test names its own.
pub const PLAN: &str = "runtime/supervision/plan.md";

/// One artifact declaration a state makes. §FS-rhei-library.7.2
#[derive(Clone, Copy)]
pub enum Decl {
    /// Listed under `outputs:`, which makes the state a declared writer.
    Writes(&'static str),
    /// Listed under `inputs:` as required, which makes it a declared reader.
    Reads(&'static str),
    /// Listed under `inputs:` with `optional: true`: still a declared reader.
    MayRead(&'static str),
}

pub use Decl::{MayRead, Reads, Writes};

/// A state's name and what it declares.
pub type Spec<'a> = (&'a str, &'a [Decl]);

/// One template laid out on disk: a valid standalone template whatever it is
/// later used as, because a template works at any level (§FS-rhei-library.6).
#[derive(Default)]
pub struct Template<'a> {
    /// The manifest name and the directory under the project template tier.
    pub name: &'a str,
    /// The node kind its tickets route by. `None` writes no `by_type`, which is
    /// what a part that brings states and no tickets needs: two parts routing
    /// one kind to two profiles would be a rule-1 clash of their own.
    pub kind: Option<&'a str>,
    /// Its states in walking order; the first is where its tickets start.
    pub states: &'a [Spec<'a>],
    /// States another part brings that this template's profile walks, which an
    /// including template's own machine names before its entries join.
    pub spans: &'a [&'a str],
    /// `includes:` entries, written as given (`../describe`).
    pub includes: &'a [&'a str],
    /// The manifest's `inputs:` block, verbatim; empty for none.
    pub inputs: &'a str,
    /// The ids of its tickets, each starting in the first state.
    pub tickets: &'a [&'a str],
}

impl Template<'_> {
    /// Write it into `<dir>/.agent-grounds/rhei/templates/<name>`.
    pub fn write(&self, dir: &Path) -> PathBuf {
        let template = dir.join(".agent-grounds/rhei/templates").join(self.name);
        std::fs::create_dir_all(template.join("tasks")).expect("create template");
        let mut manifest =
            format!("name: {}\nversion: 1.0.0\ndescription: The {} part\n", self.name, self.name);
        if self.inputs.is_empty() {
            manifest.push_str("inputs: []\n");
        } else {
            manifest.push_str(self.inputs);
        }
        if !self.includes.is_empty() {
            manifest.push_str("includes:\n");
            for entry in self.includes {
                manifest.push_str(&format!("  - {entry}\n"));
            }
        }
        write_fixture_file(&template, "template.yaml", &manifest);
        write_fixture_file(
            &template,
            "states.yaml",
            &machine(self.name, self.kind, self.states, self.spans),
        );
        let kinds = self.kind.map(|kind| format!("  nodeKinds:\n  - {kind}\n")).unwrap_or_default();
        write_fixture_file(
            &template,
            "index.rhei.md",
            &format!(
                "# Rhei: {name}\n**States:** {name}\n\n---\nstructure:\n  maxLevels: 3\n{kinds}---\n\n## Overview\n\nThe {name} part.\n",
                name = self.name
            ),
        );
        let first = self.states.first().map_or("completed", |(state, _)| *state);
        let heading = self.kind.map_or("Task".to_owned(), capitalized);
        for (n, id) in self.tickets.iter().enumerate() {
            write_fixture_file(
                &template.join("tasks"),
                &format!("{:03}-{id}.md", n + 1),
                &format!("### {heading} {id}: Work {id}\n**State:** {first}\n\nDo the work.\n"),
            );
        }
        template
    }
}

/// A machine whose states walk in order to `completed`, with one profile named
/// after it that also walks `spans`, and block-style `node_policy`. Block style
/// is deliberate: a flow-style `by_type` in a `--into` target makes the union
/// write YAML rhei cannot parse, a separate defect this file must not trip on.
pub fn machine(name: &str, kind: Option<&str>, states: &[Spec], spans: &[&str]) -> String {
    let mut yaml = format!("name: {name}\nversion: 1\nstates:\n");
    for (state, decls) in states {
        yaml.push_str(&format!(
            "  {state}:\n    description: The {state} step\n    instructions: |\n      Do the {state} step.\n"
        ));
        let inputs: Vec<&Decl> = decls.iter().filter(|decl| !matches!(decl, Writes(_))).collect();
        if !inputs.is_empty() {
            yaml.push_str("    inputs:\n");
            for (n, decl) in inputs.iter().enumerate() {
                let (Reads(path) | MayRead(path) | Writes(path)) = decl;
                yaml.push_str(&format!(
                    "      - name: read-{n}\n        path: \"{path}\"\n        description: What {state} reads\n"
                ));
                if matches!(decl, MayRead(_)) {
                    yaml.push_str("        optional: true\n");
                }
            }
        }
        let outputs: Vec<&str> = decls
            .iter()
            .filter_map(|decl| if let Writes(path) = decl { Some(*path) } else { None })
            .collect();
        if !outputs.is_empty() {
            yaml.push_str("    outputs:\n");
            for (n, path) in outputs.iter().enumerate() {
                yaml.push_str(&format!(
                    "      - name: write-{n}\n        path: \"{path}\"\n        description: What {state} writes\n"
                ));
            }
        }
    }
    yaml.push_str(
        "  completed:\n    final: true\n    description: Done\n  cancelled:\n    final: true\n    description: Abandoned\ntransitions:\n",
    );
    let names: Vec<&str> = states.iter().map(|(state, _)| *state).collect();
    for (from, to) in names.iter().zip(names.iter().skip(1).chain(["completed"].iter())) {
        yaml.push_str(&format!("  - from: {from}\n    to: {to}\n"));
    }
    yaml.push_str("  - from: \"*\"\n    to: cancelled\nprofiles:\n");
    let allowed: Vec<&str> =
        names.iter().chain(spans.iter()).copied().chain(["completed", "cancelled"]).collect();
    yaml.push_str(&format!(
        "  {name}:\n    initial: {}\n    allowed: [{}]\n",
        names.first().copied().unwrap_or("completed"),
        allowed.join(", ")
    ));
    yaml.push_str(&format!("node_policy:\n  root: {name}\n  default: {name}\n"));
    if let Some(kind) = kind {
        yaml.push_str(&format!("  by_type:\n    {kind}: {name}\n"));
    }
    yaml
}

/// A `--into` target: the shared host workspace with its machine replaced by
/// `states`, the first of which must be `pending`, where the host's own
/// ticket sits. Returns (temp dir, the rhei root).
pub fn target_with(prefix: &str, states: &[Spec]) -> (TestDir, PathBuf) {
    let (dir, root) = host_workspace(prefix);
    assert_eq!(states.first().map(|(state, _)| *state), Some("pending"), "the host ticket's state");
    write_fixture_file(&root, "states.yaml", &machine("host", Some("task"), states, &[]));
    (dir, root)
}

/// The template a `--into` places: one `step` ticket, so the two-tickets
/// warning of §FS-rhei-library.7.2.5 stays out of what a test reads.
pub fn placed(dir: &Path, name: &str, states: &[Spec]) -> PathBuf {
    Template { name, kind: Some("step"), states, tickets: &["work"], ..Template::default() }
        .write(dir)
}

/// An `includes:` part: states and no tickets, so it routes nothing.
pub fn part(dir: &Path, name: &str, states: &[Spec]) -> PathBuf {
    Template { name, states, ..Template::default() }.write(dir)
}

/// The including template every `includes:` case lays: its own supervisor
/// state, one `task` ticket, a profile spanning what its entries bring, and the
/// entries themselves.
pub fn ticket_host(dir: &Path, supervising: &[Decl], spans: &[&str], includes: &[&str]) -> PathBuf {
    Template {
        name: "ticket-host",
        kind: Some("task"),
        states: &[("supervising", supervising)],
        spans,
        includes,
        tickets: &["ticket"],
        ..Template::default()
    }
    .write(dir)
}

/// Every stderr line the shared-input warning of §FS-rhei-library.7.2.3 printed.
pub fn shared_input_warnings(result: &CliRun) -> Vec<&str> {
    result.stderr.lines().filter(|line| line.starts_with("warning: shared input")).collect()
}

/// The shared-input warning, word for word, for `path` and its sorted readers.
pub fn shared_input_line(path: &str, readers: &[&str]) -> String {
    format!(
        "warning: shared input '{path}' has no declared output producer; readers: {}; it may be \
         supplied by a program, callback, operator or instructions.",
        readers.join(", ")
    )
}

/// Establish only the scope and named destination sidecars before a refusal
/// snapshot. These empty regular files remain after failed placement, while
/// every authored byte must stay unchanged. §FS-rhei-new.4 §FS-rhei-library.2
pub fn prepare_placement_sidecars(root: &Path, destination_sidecars: &[&str]) {
    for relative in
        std::iter::once("index.rhei.md.lock").chain(destination_sidecars.iter().copied())
    {
        let path = root.join(relative);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => panic!("create sidecar {}: {err}", path.display()),
        }
        let metadata = std::fs::symlink_metadata(&path).expect("inspect permanent sidecar");
        assert!(metadata.is_file(), "sidecar must be a regular file: {}", path.display());
        assert_eq!(metadata.len(), 0, "sidecar must be empty: {}", path.display());
    }
}

/// Every file under `root` with its bytes, for a byte-for-byte comparison.
pub fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read the target") {
            let path = entry.expect("read an entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("read a target file");
                files.push((path, bytes));
            }
        }
    }
    files.sort();
    files
}

fn capitalized(kind: &str) -> String {
    let mut chars = kind.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}
