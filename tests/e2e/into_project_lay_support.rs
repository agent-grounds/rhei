//! Fixtures for laying a **Panta project** from a template: the project
//! template, the member it lays, the projects it is laid into, and a snapshot
//! that says whether a refusal wrote anything.
//!
//! The project template carries `index.panta.md` and a machine instead of a
//! plan (§FS-rhei-templates.2), so it is the third layout; the member it
//! includes carries a machine of its own, so the project default never governs
//! it and a rebind never has a reason to touch it. §FS-rhei-templates.6.4

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::into_support::BORROWED_AGENT_SETTINGS;
use super::*;

/// A machine with one working state and the two terminals, routing `kind`
/// through a profile named after the machine. The machines a project runs
/// under before a rebind, and the members' own, are all this shape.
pub fn machine(name: &str, working: &str, kind: &str) -> String {
    format!(
        r#"name: {name}
version: 1
states:
  {working}:
    description: Do the work
    instructions: |
      Do the work and move on.
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: {working}
    to: completed
  - from: "*"
    to: cancelled
profiles:
  {name}:
    initial: {working}
    allowed: [{working}, completed, cancelled]
node_policy:
  root: {name}
  default: {name}
  by_type:
    {kind}: {name}
"#
    )
}

/// The machine a project template lays as the project default. Its one
/// working state is `shipped`, which no machine a project here already runs
/// under has, and it names the bundled prompt fragment, so the bundle has to
/// land beside it for the laid project to validate. `extra_kinds` are further
/// `by_type` keys, for the node-kind seam of §FS-rhei-library.2.3.
pub fn laid_machine(extra_kinds: &[&str]) -> String {
    let mut by_type = String::from("    task: laidmachine\n");
    for kind in extra_kinds {
        by_type.push_str(&format!("    {kind}: laidmachine\n"));
    }
    format!(
        r#"name: laidmachine
version: 1
states:
  shipped:
    description: Ship it
    prompt_template:
      name: brief
      values:
        subject: the release
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: shipped
    to: completed
  - from: "*"
    to: cancelled
profiles:
  laidmachine:
    initial: shipped
    allowed: [shipped, completed, cancelled]
node_policy:
  root: laidmachine
  default: laidmachine
  by_type:
{by_type}"#
    )
}

/// The rhei-layout template every project template here includes. It declares
/// no `**States:**` line: its own root's `states.yaml` is its machine without
/// one (§FS-rhei-plan-language.1.3 clause 1).
pub fn write_intake_template(dir: &Path) -> PathBuf {
    let template = templates_home(dir).join("intake");
    std::fs::create_dir_all(template.join("tasks")).expect("create the member template");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: intake\nversion: 1.0.0\ndescription: Record what arrived\n",
    );
    write_fixture_file(&template, "states.yaml", &machine("intakemachine", "recording", "step"));
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Intake\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - step\n---\n\n## Overview\n\nRecords what arrived.\n",
    );
    write_fixture_file(
        &template.join("tasks"),
        "001-record.md",
        "### Step record: Record the report\n**State:** recording\n\nRecord it.\n",
    );
    template
}

/// A project template named `name`: a manifest with no `**States:**` line,
/// `machine` as the default it lays, a copied bundle of one prompt fragment
/// and one script, the settings it needs, and `includes` verbatim as the
/// manifest's `includes:` block (empty for none).
pub fn write_project_template(dir: &Path, name: &str, machine: &str, includes: &str) -> PathBuf {
    let template = templates_home(dir).join(name);
    std::fs::create_dir_all(template.join("prompt_templates")).expect("create the template");
    std::fs::create_dir_all(template.join("scripts")).expect("create the template");
    write_fixture_file(
        &template,
        "template.yaml",
        &format!("name: {name}\nversion: 1.0.0\ndescription: A project lifecycle\n{includes}"),
    );
    write_fixture_file(
        &template,
        "index.panta.md",
        "# Panta: Lifecycle\n\n## Overview\n\nA project laid from a template.\n",
    );
    write_fixture_file(&template, "states.yaml", machine);
    write_fixture_file(
        &template.join("prompt_templates"),
        "brief.md",
        "Write the brief for {subject}.\n",
    );
    write_fixture_file(&template.join("scripts"), "collect.sh", "#!/bin/sh\nexit 0\n");
    write_fixture_file(&template, "settings.json", BORROWED_AGENT_SETTINGS);
    template
}

/// The `lifecycle` project template and the `intake` member it includes:
/// the shape every laying test starts from.
pub fn write_lifecycle_templates(dir: &Path) -> PathBuf {
    write_intake_template(dir);
    write_project_template(dir, "lifecycle", &laid_machine(&[]), "includes:\n  - intake\n")
}

fn templates_home(dir: &Path) -> PathBuf {
    dir.join(".agent-grounds/rhei/templates")
}

/// An existing Panta project at `<dir>/<name>` whose manifest is `manifest`,
/// running under `default` when one is given. Returns its root.
pub fn write_project(dir: &Path, name: &str, manifest: &str, default: Option<&str>) -> PathBuf {
    let root = dir.join(name);
    std::fs::create_dir_all(&root).expect("create the project");
    write_fixture_file(&root, "index.panta.md", manifest);
    if let Some(default) = default {
        write_fixture_file(&root, "states.yaml", default);
    }
    root
}

/// A manifest that declares no machine, which is the ordinary case.
pub const PLAIN_MANIFEST: &str =
    "# Panta: Reports\n\n## Overview\n\nThe project a template is laid into.\n";

/// A directory-workspace member at `<project>/<id>` with one ticket. `index`
/// is its whole `index.rhei.md`; `machine`, when given, is its own root's
/// `states.yaml`.
pub fn write_member(project: &Path, id: &str, index: &str, machine: Option<&str>, ticket: &str) {
    let root = project.join(id);
    std::fs::create_dir_all(root.join("tasks")).expect("create the member");
    write_fixture_file(&root, "index.rhei.md", index);
    if let Some(machine) = machine {
        write_fixture_file(&root, "states.yaml", machine);
    }
    write_fixture_file(&root.join("tasks"), "001-ticket.md", ticket);
}

/// Every file under `root`, keyed by its `/`-separated relative path.
///
/// A sidecar `*.lock` is left out: the lock a rewriting command takes is
/// permanent and holds no content (§FS-rhei-new.4), so its presence says a lock
/// was taken rather than that anything was written.
pub fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read a snapshot directory") {
            let path = entry.expect("read a snapshot entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_none_or(|ext| ext != "lock") {
                let relative = path.strip_prefix(root).expect("under the snapshot root");
                let key = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                files.insert(key, std::fs::read(&path).expect("read a snapshot file"));
            }
        }
    }
    files
}

/// Assert two snapshots hold the same files with the same bytes, naming the
/// paths that differ rather than printing both trees.
pub fn assert_same_tree(
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
    what: &str,
) {
    let differing: Vec<&String> = before
        .keys()
        .chain(after.keys())
        .filter(|key| before.get(*key) != after.get(*key))
        .collect();
    assert!(differing.is_empty(), "{what}; these paths differ: {differing:?}");
}

/// The `name:` a states file declares.
pub fn machine_name(path: &Path) -> String {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    text.lines()
        .find_map(|line| line.strip_prefix("name:"))
        .map(|name| name.trim().to_owned())
        .unwrap_or_default()
}

/// Run `rhei` from `cwd` with the home the fixture directory `dir` owns, so
/// the user template tier only holds what a test put there.
pub fn rhei_in(dir: &Path, cwd: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(dir.join(".home"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}
