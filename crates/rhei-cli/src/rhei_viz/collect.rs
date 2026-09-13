//! Collect [`VizModel`]s from a plan file or a Directory Workspace, resolving
//! each plan's machine (built-in default, sibling `states.yaml`, or override).
//! The static-path collection shared by `rhei viz` and the `xtask` dogfood. §FS-rhei-viz.7.2

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::rhei_validator::StateMachine;
use crate::rhei_viz_model::VizModel;
use rhei_core::ast::Rhei;
use rhei_core::{parse, workspace};

use crate::rhei_viz::build_with_history;

/// A keyed bundle of plans in the shape `rhei-viz-model::render_static` inlines
/// and the asset's plan selector reads.
pub type Bundle = BTreeMap<String, VizModel>;

/// Collect every plan reachable from `path` into a render-ready bundle.
///
/// - a file → one plan, keyed by `key`;
/// - a Directory Workspace → the merged `index.rhei.md` (keyed by `key`) plus
///   each standalone `*.rhei.md` (keyed `key::<relative-path>`).
///
/// `machine_override`, when set, resolves every plan against that YAML file;
/// otherwise each plan resolves the built-in default (`**States:** rhei`) or a
/// sibling `states.yaml`.
pub fn collect_plans(
    path: &Path,
    key: &str,
    machine_override: Option<&Path>,
) -> io::Result<Bundle> {
    let mut plans = Bundle::new();

    if path.is_file() {
        let model = load_plan_file(path, machine_override)?;
        plans.insert(key.to_string(), model);
        return Ok(plans);
    }

    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no such file or directory: {}", path.display()),
        ));
    }

    if workspace::is_panta_project(path) {
        // A project renders as one graph: cross-rhei `**Prior:**` edges are
        // drawn, and Directory Workspace rheis appear alongside single-file
        // ones. §FS-rhei-viz.7.3
        let loaded = workspace::load_panta_project(path).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to load Panta project {}: {}", path.display(), err.message),
            )
        })?;
        let machines = resolve_project_machines(path, machine_override, &loaded)?;
        // Runtime ledgers live under each rhei's own root, not the project's.
        // §AR-rhei-panta.5
        let task_roots = loaded
            .task_roots
            .iter()
            .map(|(id, root)| (id.clone(), root.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        plans.insert(
            key.to_string(),
            crate::rhei_viz::build_set_with_history_roots(
                &loaded.rhei,
                &machines,
                path,
                &task_roots,
            ),
        );
        return Ok(plans);
    }

    if workspace::is_workspace(path) {
        // §AR-rhei-panta.2: viz renders the same implicit-Panta id space as
        // the CLI, so workspace tickets carry their qualified ids.
        let loaded = workspace::load_implicit_panta(path).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to load workspace {}: {}", path.display(), err.message),
            )
        })?;
        let machine = resolve_machine(path, machine_override, &loaded.rhei)?;
        plans.insert(key.to_string(), build_with_history(&loaded.rhei, &machine, path));
    }

    for plan_path in standalone_plan_files(path)? {
        if plan_path.file_name().and_then(|name| name.to_str()) == Some("index.rhei.md") {
            continue;
        }
        let rel = plan_path.strip_prefix(path).unwrap_or(&plan_path).to_string_lossy().to_string();
        let plan_key = format!("{key}::{rel}");
        plans.insert(plan_key, load_plan_file(&plan_path, machine_override)?);
    }

    Ok(plans)
}

fn load_plan_file(path: &Path, machine_override: Option<&Path>) -> io::Result<VizModel> {
    let text = fs::read_to_string(path)?;
    let rhei = parse(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {}: {}", path.display(), err.message),
        )
    })?;
    // §AR-rhei-panta.2: viz renders the implicit-Panta id space (`auth.1`).
    let rhei = workspace::implicit_panta_from_file_rhei(rhei, path)
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to load {}: {}", path.display(), err.message),
            )
        })?
        .rhei;
    let machine = resolve_machine(path, machine_override, &rhei)?;
    let workspace_root = path.parent().unwrap_or_else(|| Path::new("."));
    Ok(build_with_history(&rhei, &machine, workspace_root))
}

/// Resolve every machine a project's rheis run under: the manifest default via
/// [`resolve_machine`], plus each self-declaring rhei's machine from its own
/// execution root (falling back to the project root). Mirrors CLI resolution.
// §DA-per-rhei-state-machines §AR-rhei-panta.4
fn resolve_project_machines(
    path: &Path,
    machine_override: Option<&Path>,
    loaded: &workspace::PantaProject,
) -> io::Result<crate::rhei_validator::MachineSet> {
    let default = resolve_machine(path, machine_override, &loaded.rhei)?;
    let mut per_rhei = std::collections::BTreeMap::new();
    let mut declared: Vec<(&String, &String)> = loaded.rhei_machines.iter().collect();
    declared.sort();
    for (rhei_id, machine_name) in declared {
        if machine_override.is_some() {
            if *machine_name != default.name {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "--states declares '{}', but rhei '{rhei_id}' declares state machine                      '{machine_name}'; the override cannot reinterpret that rhei's states",
                        default.name
                    ),
                ));
            }
            continue;
        }

        if let Some(root) = loaded.rhei_roots.get(rhei_id) {
            let candidate = root.join("states.yaml");
            if candidate.is_file() {
                // Static collection gives every explicit declaration local
                // first refusal, including a repeated default name. §FS-rhei-plan-language.1.3
                let machine = load_machine(&candidate)?;
                if machine.name == *machine_name {
                    per_rhei.insert(rhei_id.clone(), machine);
                    continue;
                }
            }
        }
        if *machine_name == default.name {
            continue;
        }

        let candidates = [path.join("states.yaml")];
        let mut resolved = None;
        for candidate in candidates {
            if !candidate.is_file() {
                continue;
            }
            let machine = load_machine(&candidate)?;
            if machine.name == *machine_name {
                resolved = Some(machine);
                break;
            }
        }
        let Some(machine) = resolved else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "rhei '{rhei_id}' declares state machine '{machine_name}', but no states                      file declaring it was found in its root or the project root"
                ),
            ));
        };
        per_rhei.insert(rhei_id.clone(), machine);
    }
    Ok(crate::rhei_validator::MachineSet { default, per_rhei })
}

/// Resolve the state machine for a plan: an explicit `--states` override wins,
/// then a matching sibling `states.yaml` next to the plan (or workspace root),
/// then the built-in default for `**States:** rhei`.
fn resolve_machine(
    plan_or_dir: &Path,
    machine_override: Option<&Path>,
    rhei: &Rhei,
) -> io::Result<StateMachine> {
    if let Some(machine_path) = machine_override {
        return load_machine(machine_path);
    }
    let dir = if plan_or_dir.is_dir() {
        plan_or_dir.to_path_buf()
    } else {
        plan_or_dir.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    };
    let candidate = dir.join("states.yaml");
    if candidate.is_file() {
        // Static viz mirrors CLI state-machine discovery: a matching local file
        // overrides the built-in `rhei` machine. §FS-rhei-viz.8
        let machine = load_machine(&candidate)?;
        if machine.name == rhei.states {
            return Ok(machine);
        }
        let builtin = StateMachine::builtin_default();
        if rhei.states != builtin.name {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "plan declares state machine '{}', but auto-discovered states file '{}' declares '{}'",
                    rhei.states,
                    candidate.display(),
                    machine.name
                ),
            ));
        }
        return Ok(builtin);
    }
    if rhei.states == StateMachine::builtin_default().name {
        return Ok(StateMachine::builtin_default());
    }
    load_machine(&candidate)
}

fn load_machine(machine_path: &Path) -> io::Result<StateMachine> {
    StateMachine::from_yaml_file(machine_path).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to load state machine {}: {err}", machine_path.display()),
        )
    })
}

fn standalone_plan_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".rhei.md"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
            let path = std::env::temp_dir().join(format!("rhei-viz-collect-{prefix}-{stamp}"));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
        fn path(&self) -> &Path {
            &self.path
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn merges_workspace_and_standalone_plans() {
        let temp = TempDir::new("ws");
        fs::write(
            temp.path().join("index.rhei.md"),
            "# Rhei: Workspace\n**States:** rhei\n\n## Overview\nDemo.\n",
        )
        .unwrap();
        fs::create_dir_all(temp.path().join("tasks")).unwrap();
        fs::write(
            temp.path().join("tasks/alpha.rhei.md"),
            "### Task 1: Alpha\n**State:** pending\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("extra.rhei.md"),
            "# Rhei: Extra\n**States:** rhei\n\n## Tasks\n\n### Task 1: Extra\n**State:** completed\n",
        )
        .unwrap();

        let plans = collect_plans(temp.path(), "demo", None).expect("collect");
        assert_eq!(plans.len(), 2);
        assert_eq!(plans["demo"].plan_title.as_deref(), Some("Workspace"));
        assert_eq!(plans["demo::extra.rhei.md"].plan_title.as_deref(), Some("Extra"));
    }

    #[test]
    fn single_file_resolves_builtin_default() {
        let temp = TempDir::new("file");
        let plan = temp.path().join("plan.rhei.md");
        fs::write(
            &plan,
            "# Rhei: Solo\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** in-progress\n",
        )
        .unwrap();
        let plans = collect_plans(&plan, "solo", None).expect("collect");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans["solo"].plan_state.as_deref(), Some("active"));
        assert!(!plans["solo"].machine.states.is_empty());
    }

    #[test]
    fn history_falls_back_to_legacy_rhei_local_ids() {
        let temp = TempDir::new("legacy-history");
        let plan = temp.path().join("plan.rhei.md");
        fs::write(
            &plan,
            "# Rhei: Legacy\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** in-progress\n",
        )
        .unwrap();
        let runtime = temp.path().join("runtime");
        fs::create_dir_all(&runtime).unwrap();
        // A ledger written before project qualification keys records by the
        // rhei-local id, not `plan.1`. §AR-rhei-panta.2
        fs::write(runtime.join("state-transitions.log"), "1 pending@in-progress\n").unwrap();

        let plans = collect_plans(&plan, "legacy", None).expect("collect");
        let task = &plans["legacy"].tasks[0];
        assert_eq!(task.id, "plan.1");
        assert_eq!(task.history.len(), 1, "legacy local-id history should attach");
        assert_eq!(task.history[0].from, "pending");
        assert_eq!(task.history[0].to, "in-progress");

        // A qualified record wins outright; the legacy key is a fallback,
        // never a merge source.
        fs::write(
            runtime.join("state-transitions.log"),
            "plan.1 pending@completed\n1 pending@in-progress\n",
        )
        .unwrap();
        let plans = collect_plans(&plan, "legacy", None).expect("collect");
        let task = &plans["legacy"].tasks[0];
        assert_eq!(task.history.len(), 1);
        assert_eq!(task.history[0].to, "completed");
    }

    #[test]
    fn sibling_states_named_rhei_overrides_builtin_default() {
        let temp = TempDir::new("local-rhei");
        let plan = temp.path().join("plan.rhei.md");
        fs::write(
            &plan,
            "# Rhei: Local\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** local-work\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("states.yaml"),
            r#"
name: rhei
version: 1.0
states:
  local-work:
    initial: true
    instructions: "Use the local machine."
  completed:
    final: true
transitions:
  - from: local-work
    to: completed
"#,
        )
        .unwrap();

        let plans = collect_plans(&plan, "local", None).expect("collect");
        let local = plans["local"]
            .machine
            .states
            .iter()
            .find(|state| state.name == "local-work")
            .expect("local state is rendered");
        assert_eq!(local.instructions.as_deref(), Some("Use the local machine."));
    }

    /// A member row is normalized through its matching local definition even
    /// when that definition has the project default's name.
    // §FS-rhei-plan-language.1.3
    #[test]
    fn same_name_member_row_uses_its_local_machine() {
        let temp = TempDir::new("same-name-member");
        fs::write(temp.path().join("index.panta.md"), "# Panta: Billing\n**States:** alpha\n")
            .unwrap();
        fs::write(
            temp.path().join("states.yaml"),
            "name: alpha\nversion: 1\nstates:\n  surveying:\n    initial: true\n  signed-off:\n    final: true\ntransitions:\n  - from: surveying\n    to: signed-off\n",
        )
        .unwrap();
        let billing = temp.path().join("billing");
        fs::create_dir_all(billing.join("tasks")).unwrap();
        fs::write(billing.join("index.rhei.md"), "# Rhei: Billing\n**States:** alpha\n").unwrap();
        fs::write(
            billing.join("states.yaml"),
            "name: alpha\nversion: 1\nstates:\n  drafting:\n    initial: true\n    visits: 3\n  filed:\n    final: true\ntransitions:\n  - from: drafting\n    to: drafting\n  - from: drafting\n    to: filed\n",
        )
        .unwrap();
        fs::write(billing.join("tasks/01.md"), "### Task 1: Draft\n**State:** drafting-2\n")
            .unwrap();

        let plans = collect_plans(temp.path(), "project", None).expect("collect project");
        let row =
            plans["project"].tasks.iter().find(|task| task.id == "billing.1").expect("billing row");
        assert_eq!(row.state, "drafting");
        assert_eq!(row.visit_count, Some(2));
        assert!(
            plans["project"].machine.states.iter().any(|state| state.name == "drafting"),
            "the rendered machine union should include the member-local process"
        );
    }
}
