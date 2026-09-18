//! The Panta project: many rheis under one directory, merged into one graph.
//!
//! A rhei is loaded on its own by [`super::load_workspace`] or the single-file
//! parser. A project adds discovery (which entries are rheis, and what id each
//! one answers to), a synthetic `basin` for tickets that belong to no rhei, and
//! one merge that folds every rhei's tickets into a single plan with qualified
//! ids while keeping each rhei's own machine, execution root, title, and plan
//! document reachable.
//!
//! Its own module because loading one rhei can fail on a file and loading a
//! project can fail on a rhei — a lenient load skips the second and never the
//! first.

// §AR-rhei-panta §FS-rhei-panta

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{ContentSection, Rhei};
use crate::parser::{self, ParseError};

use super::basin::{basin_id_reserved_error, load_basin_rhei, BASIN_RHEI_ID};
use super::qualify::{
    collect_task_ids, merge_structure, merge_task_metadata, qualify_task_metadata, qualify_tasks,
    rhei_id_for_entry, validate_rhei_id,
};
use super::{
    collect_task_roots, collect_task_sources, is_workspace, load_workspace, nested_parse_error,
    plan_parent_dir, source_for_task, workspace_dir, Workspace, RHEI_INDEX_FILE,
};

pub const PANTA_INDEX_FILE: &str = "index.panta.md";

/// A loaded Panta project, flattened into a project-qualified task graph for
/// the existing task execution pipeline. §AR-rhei-panta.2 §AR-rhei-panta.3
#[derive(Debug)]
pub struct PantaProject {
    /// Retain every project/member lock through consumers. §FS-rhei-panta.6.6
    pub root_guards: Vec<crate::root_access::RootAccessGuard>,
    pub rhei: Rhei,
    /// Maps project-qualified task ID (`auth.1`) → the file path that defines it.
    pub task_sources: HashMap<String, PathBuf>,
    /// Maps project-qualified task ID (`auth.1`) → the owning rhei execution root. §AR-rhei-panta.5
    pub task_roots: HashMap<String, PathBuf>,
    /// Link-validation base directory for each merged content section, in
    /// `rhei.content_sections` order. §AR-rhei-panta.5
    pub content_section_roots: Vec<PathBuf>,
    /// Rhei ids in presentation order; `basin` is always last when present.
    pub rhei_ids: Vec<String>,
    /// State-machine name each rhei declared with its own `**States:**` line.
    /// Absent for rheis that declare nothing — they run the project default.
    /// The merge records ownership instead of discarding it, so every consumer
    /// can resolve a ticket's machine through its owning rhei.
    // §DA-per-rhei-state-machines: the machine is per-rhei, defaulted by the manifest.
    pub rhei_machines: HashMap<String, String>,
    /// Execution root of each rhei, keyed by rhei id — where a self-declared
    /// machine's `states.yaml` resolves first. §AR-rhei-panta.4
    pub rhei_roots: HashMap<String, PathBuf>,
    /// Title each rhei declared in its own `# Rhei:` heading, keyed by rhei id.
    ///
    /// The merge folds every rhei's tickets into one graph and keeps only the
    /// project's title on `rhei.title`, so a reader that has to name the rhei a
    /// ticket came from had nowhere to look.
    // §FS-rhei-memory.3.1
    pub rhei_titles: HashMap<String, String>,
    /// Plan document of each rhei, keyed by rhei id: a Directory Workspace's
    /// `index.rhei.md`, or the single-file rhei itself. §FS-rhei-memory.3.4
    pub rhei_plans: HashMap<String, PathBuf>,
    /// Rheis skipped by a lenient load, one message each. Always empty for the
    /// strict load, which fails on the first unloadable rhei instead.
    pub unloadable: Vec<String>,
}

/// Returns `true` if `path` is a Panta project directory.
pub fn is_panta_project(path: &Path) -> bool {
    path.is_dir() && path.join(PANTA_INDEX_FILE).is_file()
}

/// Resolve a Panta project directory from either the project directory or its
/// `index.panta.md` manifest path. §FS-rhei-panta.6
pub fn panta_project_dir(path: &Path) -> Option<PathBuf> {
    if is_panta_project(path) {
        return Some(path.to_path_buf());
    }
    if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(PANTA_INDEX_FILE) {
        return path.parent().map(Path::to_path_buf);
    }
    None
}

/// Discover rhei entries among a Panta project directory's immediate children.
///
/// A rhei entry is either a direct-child `*.rhei.md` file (a Single-File Plan)
/// or a direct-child subdirectory that is a Directory Workspace (contains
/// `index.rhei.md`). Discovery does **not** descend into other subdirectories:
/// rheis live directly in the project directory, so a stray `*.rhei.md` buried
/// in, say, `docs/` is not silently promoted to a rhei. The `runtime/` artifact
/// tree and the reserved `basin/` directory are skipped here and handled
/// separately. Entries are returned in deterministic, `/`-normalized order.
pub fn discover_rhei_entries(project_dir: &Path) -> parser::Result<Vec<PathBuf>> {
    let _guards = crate::root_access::for_input(project_dir)
        .map_err(|err| ParseError::new(err.to_string(), None))?;
    let mut entries = Vec::new();
    if !project_dir.is_dir() {
        return Ok(entries);
    }

    let read = std::fs::read_dir(project_dir).map_err(|e| {
        ParseError::new(format!("failed to read {}: {e}", project_dir.display()), None)
    })?;
    for entry in read {
        let entry = entry.map_err(|e| {
            ParseError::new(format!("failed to read {}: {e}", project_dir.display()), None)
        })?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type().map_err(|e| {
            ParseError::new(format!("failed to inspect {}: {e}", path.display()), None)
        })?;
        if file_type.is_dir() {
            // The `runtime/` artifact tree and the synthetic `basin/` are not
            // discoverable domain rheis; a non-workspace subdirectory is not a
            // rhei either, as rheis are not nested in grouping folders. §AR-rhei-panta.1
            if name == "runtime" || name == BASIN_RHEI_ID {
                continue;
            }
            if is_workspace(&path) {
                entries.push(path);
            }
        } else if file_type.is_file() && name.ends_with(".rhei.md") {
            entries.push(path);
        }
    }

    entries.sort_by(|a, b| {
        let a_key = a.strip_prefix(project_dir).unwrap_or(a).to_string_lossy().replace('\\', "/");
        let b_key = b.strip_prefix(project_dir).unwrap_or(b).to_string_lossy().replace('\\', "/");
        a_key.cmp(&b_key)
    });
    Ok(entries)
}

/// Load a Panta project, merging all contained rheis into one graph with
/// project-qualified task ids. §AR-rhei-panta.2 §AR-rhei-panta.3
pub fn load_panta_project(dir: &Path) -> parser::Result<PantaProject> {
    load_panta_project_with(dir, false, None)
}

/// Load a project as if `entry` were already published under `rhei_id`.
/// Template instantiation uses this overlay while the bytes remain in a hidden
/// same-parent staging directory. §AR-rhei-panta.2 §AR-rhei-panta.4
pub fn load_panta_project_with_member(
    dir: &Path,
    rhei_id: &str,
    entry: &Path,
) -> parser::Result<PantaProject> {
    load_panta_project_with(dir, false, Some((rhei_id, entry)))
}

/// Load a project, skipping rheis that fail to load instead of failing the
/// whole project, and recording why in [`PantaProject::unloadable`].
/// §FS-rhei-panta.6
pub fn load_panta_project_lenient(dir: &Path) -> parser::Result<PantaProject> {
    load_panta_project_with(dir, true, None)
}

fn load_panta_project_with(
    dir: &Path,
    lenient: bool,
    prospective: Option<(&str, &Path)>,
) -> parser::Result<PantaProject> {
    let root_guards =
        crate::root_access::for_input(dir).map_err(|err| ParseError::new(err.to_string(), None))?;
    let manifest_path = dir.join(PANTA_INDEX_FILE);
    let manifest_content = crate::source::read_to_string(&manifest_path).map_err(|e| {
        ParseError::new(format!("failed to read {}: {e}", manifest_path.display()), None)
    })?;
    let manifest = parser::parse_panta_manifest(&manifest_content)
        .map_err(|e| nested_parse_error(e, &manifest_path))?;

    let mut rheis = Vec::new();
    let mut unloadable: Vec<String> = Vec::new();
    let mut seen_ids: HashMap<String, PathBuf> = HashMap::new();
    let mut entries: Vec<(PathBuf, Option<String>)> =
        discover_rhei_entries(dir)?.into_iter().map(|entry| (entry, None)).collect();
    if let Some((id, entry)) = prospective {
        entries.push((entry.to_path_buf(), Some(id.to_string())));
    }
    // The prospective entry occupies its final id's canonical position even
    // though its staging pathname is deliberately undiscoverable.
    entries.sort_by(|(a_path, a_id), (b_path, b_id)| {
        let key = |path: &Path, id: &Option<String>| {
            id.clone().unwrap_or_else(|| {
                path.strip_prefix(dir).unwrap_or(path).to_string_lossy().replace('\\', "/")
            })
        };
        key(a_path, a_id).cmp(&key(b_path, b_id))
    });
    for (entry, prospective_id) in entries {
        // An unusable *id* — malformed, reserved, or already taken — keeps an
        // entry out of the project exactly as a parse failure does, so a
        // lenient load skips it the same way. §FS-rhei-panta.6
        let id = match prospective_id.map(Ok).unwrap_or_else(|| rhei_id_for_entry(&entry)) {
            Ok(id) => id,
            Err(err) if lenient => {
                unloadable.push(format!(
                    "{} could not be loaded: {}",
                    entry.display(),
                    err.message
                ));
                continue;
            }
            Err(err) => return Err(err),
        };
        let id_conflict = match validate_rhei_id(&id, &entry) {
            Ok(()) if id == BASIN_RHEI_ID => Some(basin_id_reserved_error(&entry)),
            Ok(()) => seen_ids.get(&id).map(|first| {
                ParseError::new(
                    format!(
                        "duplicate rhei id '{id}' in Panta project: derived from both {} and {}. \
                         Rename one of them — the id comes from the file stem or directory name",
                        first.display(),
                        entry.display()
                    ),
                    None,
                )
            }),
            Err(err) => Some(err),
        };
        if let Some(err) = id_conflict {
            if !lenient {
                return Err(err);
            }
            unloadable.push(format!("rhei '{id}' could not be loaded: {}", err.message));
            continue;
        }
        seen_ids.insert(id.clone(), entry.clone());
        let root = rhei_execution_root(&entry);
        // A rhei's own `**States:**` declaration is recorded, not policed:
        // the machine is a per-rhei property defaulted by the manifest.
        // §DA-per-rhei-state-machines §AR-rhei-panta.4
        let entry_result = load_rhei_entry(&entry);
        let loaded = match entry_result {
            Ok(loaded) => loaded,
            Err(err) if lenient => {
                seen_ids.remove(&id);
                let where_ = match err.line {
                    Some(line) => format!("{}:{line}", entry.display()),
                    None => entry.display().to_string(),
                };
                unloadable
                    .push(format!("rhei '{id}' could not be loaded ({where_}): {}", err.message));
                continue;
            }
            Err(err) => return Err(err),
        };
        rheis.push((id, loaded.rhei, loaded.task_sources, root, entry));
    }

    let basin_dir = dir.join(BASIN_RHEI_ID);
    if basin_dir.is_dir() {
        if seen_ids.insert(BASIN_RHEI_ID.to_string(), basin_dir.clone()).is_some() {
            return Err(ParseError::new("duplicate synthetic basin rhei id", None));
        }
        let loaded = load_basin_rhei(&basin_dir, &manifest.structure, &manifest.states)?;
        rheis.push((
            BASIN_RHEI_ID.to_string(),
            loaded.rhei,
            loaded.task_sources,
            basin_dir.clone(),
            basin_dir,
        ));
    }

    let rhei_ids: Vec<String> = rheis.iter().map(|(id, ..)| id.clone()).collect();
    let mut all_tasks = Vec::new();
    let mut task_sources = HashMap::new();
    let mut task_roots = HashMap::new();
    let mut rhei_machines: HashMap<String, String> = HashMap::new();
    let mut rhei_roots: HashMap<String, PathBuf> = HashMap::new();
    let mut rhei_titles: HashMap<String, String> = HashMap::new();
    let mut rhei_plans: HashMap<String, PathBuf> = HashMap::new();
    let mut merged_structure = manifest.structure.clone();
    let mut merged_metadata = manifest.metadata.clone();
    let mut content_sections = manifest.content_sections.clone();
    let mut content_section_roots = vec![dir.to_path_buf(); content_sections.len()];
    for (rhei_id, mut rhei, sources, root, entry) in rheis {
        // Machine ownership survives the merge: a declared `**States:**` is
        // the rhei's own machine; silence means the project default. The
        // synthetic basin is built on the manifest machine and records no
        // declaration.

        // §DA-per-rhei-state-machines
        if rhei.states_declared && rhei_id != BASIN_RHEI_ID {
            rhei_machines.insert(rhei_id.clone(), rhei.states.trim().to_string());
        }
        rhei_roots.insert(rhei_id.clone(), root.clone());
        // §FS-rhei-memory.3.1 §FS-rhei-memory.3.4: the merged graph keeps the
        // project's title and paths; a prompt that names the owning rhei needs
        // the rhei's own.
        rhei_titles.insert(rhei_id.clone(), rhei.title.clone());
        if let Some(plan) = rhei_plan_file(&entry) {
            rhei_plans.insert(rhei_id.clone(), plan);
        }
        // Child runtime metadata joins the merged graph under qualified keys
        // so counted loops and poll timers resolve project-wide. §AR-rhei-panta.2
        merge_task_metadata(
            &mut merged_metadata,
            qualify_task_metadata(rhei.metadata.take(), &rhei_id),
        );
        merge_structure(&mut merged_structure, &rhei.structure);
        content_sections.push(ContentSection {
            title: format!("Rhei {rhei_id}: {}", rhei.title),
            content: String::new(),
            rhei: Some(rhei_id.clone()),
        });
        content_section_roots.push(root.clone());
        for section in &rhei.content_sections {
            content_sections.push(ContentSection {
                title: format!("Rhei {rhei_id} / {}", section.title),
                content: section.content.clone(),
                rhei: Some(rhei_id.clone()),
            });
            content_section_roots.push(root.clone());
        }
        let local_ids = collect_task_ids(&rhei.tasks);
        qualify_tasks(&mut rhei.tasks, &rhei_id, &local_ids);
        for task in &rhei.tasks {
            let source = source_for_task(&sources, task)?;
            collect_task_sources(task, source.as_path(), &mut task_sources)?;
            collect_task_roots(task, &root, &mut task_roots)?;
        }
        all_tasks.extend(rhei.tasks);
    }

    // An empty project — a manifest with no rheis yet — is a valid project;
    // `rhei init` creates exactly this state. §FS-rhei-panta.6
    Ok(PantaProject {
        root_guards,
        rhei: Rhei {
            title: manifest.title,
            states: manifest.states,
            states_declared: manifest.states_declared,
            structure: merged_structure,
            metadata: merged_metadata,
            content_sections,
            tasks: all_tasks,
        },
        task_sources,
        task_roots,
        content_section_roots,
        rhei_ids,
        rhei_machines,
        rhei_roots,
        rhei_titles,
        rhei_plans,
        unloadable,
    })
}

fn rhei_execution_root(path: &Path) -> PathBuf {
    // Not `path.parent()` raw: a bare relative plan name has an empty parent,
    // and that root reaches `RHEI_ROOT` and every path a prompt prints.
    // §FS-rhei-memory.3.4
    workspace_dir(path).unwrap_or_else(|| plan_parent_dir(path).to_path_buf())
}

/// Load a bare rhei (single `.rhei.md` file or Directory Workspace) as the
/// single rhei of an implicit Panta: same graph shape as an explicit project,
/// no manifest, ids derived from the source location. §AR-rhei-panta.2
pub fn load_implicit_panta(path: &Path) -> parser::Result<PantaProject> {
    let entry = workspace_dir(path).unwrap_or_else(|| path.to_path_buf());
    let loaded = load_rhei_entry(&entry)?;
    wrap_rhei_as_implicit_panta(loaded, &entry)
}

/// Wrap a parsed single-file rhei as its implicit Panta. §AR-rhei-panta.2
pub fn implicit_panta_from_file_rhei(rhei: Rhei, file: &Path) -> parser::Result<PantaProject> {
    let mut task_sources = HashMap::new();
    for task in &rhei.tasks {
        collect_task_sources(task, file, &mut task_sources)?;
    }
    wrap_rhei_as_implicit_panta(
        Workspace {
            rhei,
            task_sources,
            // The plan was already parsed in memory, so a synthetic source
            // path that does not exist has no filesystem read to guard.
            root_guards: if file.exists() {
                crate::root_access::for_input(file)
                    .map_err(|err| ParseError::new(err.to_string(), None))?
            } else {
                Vec::new()
            },
        },
        file,
    )
}

/// The rhei id the path `entry` names, with every rule that governs it: it is
/// derived from the resolved directory name or the file stem, must be a valid
/// single-segment id, and cannot be the reserved `basin`. §AR-rhei-panta.3
///
/// Public and separate from the wrapper below so a caller can tell an identity
/// failure — where the path is wrong and the plan is fine — from the plan
/// errors the same load reports. §FS-rhei-panta.6
pub fn rhei_id_for_path(entry: &Path) -> parser::Result<String> {
    let id = rhei_id_for_entry(entry)?;
    validate_rhei_id(&id, entry)?;
    if id == BASIN_RHEI_ID {
        return Err(basin_id_reserved_error(entry));
    }
    Ok(id)
}

/// Wrap an already-loaded bare rhei as its implicit Panta. §AR-rhei-panta.2:
/// the rhei is the sole level-1 child; §AR-rhei-panta.3: its id derives from
/// the file stem or directory name and project-qualifies every ticket.
pub fn wrap_rhei_as_implicit_panta(
    loaded: Workspace,
    entry: &Path,
) -> parser::Result<PantaProject> {
    let id = rhei_id_for_path(entry)?;
    let root = rhei_execution_root(entry);
    let mut rhei = loaded.rhei;
    let rhei_ids = vec![id.clone()];
    let local_ids = collect_task_ids(&rhei.tasks);
    qualify_tasks(&mut rhei.tasks, &id, &local_ids);
    // On-disk frontmatter keys stay rhei-local; merged-graph reads resolve
    // through project-qualified keys. §AR-rhei-panta.2
    rhei.metadata = qualify_task_metadata(rhei.metadata.take(), &id);
    let mut task_sources = HashMap::new();
    let mut task_roots = HashMap::new();
    for task in &rhei.tasks {
        let source = source_for_task(&loaded.task_sources, task)?;
        collect_task_sources(task, source.as_path(), &mut task_sources)?;
        collect_task_roots(task, &root, &mut task_roots)?;
    }
    let rhei_roots = HashMap::from([(id.clone(), root.clone())]);
    let rhei_titles = HashMap::from([(id.clone(), rhei.title.clone())]);
    let rhei_plans: HashMap<String, PathBuf> =
        rhei_plan_file(entry).into_iter().map(|plan| (id.clone(), plan)).collect();
    let content_section_roots = vec![root; rhei.content_sections.len()];
    // The implicit Panta has no manifest: the single rhei's own `**States:**`
    // declaration is the project's effective machine, so it needs no per-rhei
    // entry. §AR-rhei-panta.2
    Ok(PantaProject {
        root_guards: loaded.root_guards,
        rhei,
        task_sources,
        task_roots,
        content_section_roots,
        rhei_ids,
        rhei_machines: HashMap::new(),
        rhei_roots,
        rhei_titles,
        rhei_plans,
        unloadable: Vec::new(),
    })
}

/// The plan document of a rhei entry: a Directory Workspace's `index.rhei.md`,
/// or the single-file rhei itself. The execution root of a workspace rhei is
/// the directory, which is not a document anyone can open.
///
/// `None` when the entry is a directory with no index — the synthetic basin,
/// whose manifest is never authored — so a caller names the tickets' own files
/// instead of a plan that does not exist.
// §FS-rhei-memory.3.4 §AR-rhei-panta.1
pub fn rhei_plan_file(entry: &Path) -> Option<PathBuf> {
    if let Some(dir) = workspace_dir(entry) {
        return Some(dir.join(RHEI_INDEX_FILE));
    }
    entry.is_file().then(|| entry.to_path_buf())
}

fn load_rhei_entry(path: &Path) -> parser::Result<Workspace> {
    let _guards = crate::root_access::for_input(path)
        .map_err(|err| ParseError::new(err.to_string(), None))?;
    if let Some(ws_dir) = workspace_dir(path) {
        load_workspace(&ws_dir)
    } else {
        let content = crate::source::read_to_string(path).map_err(|e| {
            ParseError::new(format!("failed to read {}: {e}", path.display()), None)
        })?;
        let rhei = parser::parse(&content).map_err(|e| nested_parse_error(e, path))?;
        let mut task_sources = HashMap::new();
        for task in &rhei.tasks {
            collect_task_sources(task, path, &mut task_sources)?;
        }
        Ok(Workspace {
            rhei,
            task_sources,
            root_guards: crate::root_access::for_input(path)
                .map_err(|err| ParseError::new(err.to_string(), None))?,
        })
    }
}

/// Resolve `path` as a rhei entry inside a Panta project, returning the project
/// directory and the entry's rhei id.
///
/// A rhei that belongs to a project cannot be understood without it: its
/// `**Prior:**` may point across rheis and its state machine comes from the
/// manifest. Commands therefore load the project and narrow to this id, rather
/// than loading the file alone.
// §FS-rhei-panta.6: pointing at a member rhei is `--rhei <id>` on its project.
pub fn panta_member(path: &Path) -> Option<(PathBuf, String)> {
    // `index.rhei.md` inside a workspace stands for the workspace directory.
    let entry = workspace_dir(path).unwrap_or_else(|| path.to_path_buf());
    // A bare `billing.rhei.md` has no parent component, so resolve against the
    // invocation directory before asking what encloses it.
    let absolute = if entry.is_absolute() {
        entry.clone()
    } else {
        std::env::current_dir().ok()?.join(&entry)
    };
    // `Path::parent` of a path ending in `..` is the directory it climbs out of,
    // so resolve the spellings that carry no name of their own — as the id
    // derivation does — before asking what encloses the entry. §FS-rhei-panta.6
    let absolute = match absolute.file_name() {
        Some(_) => absolute,
        None => std::fs::canonicalize(&absolute).ok()?,
    };
    let parent = absolute.parent()?;
    if !is_panta_project(parent) {
        return None;
    }
    let name = absolute.file_name()?;
    if absolute.is_dir() && name == BASIN_RHEI_ID {
        return Some((parent.to_path_buf(), BASIN_RHEI_ID.to_string()));
    }
    // Only a real rhei entry qualifies: a stray `notes.md` beside the manifest
    // is not one, and neither is the `runtime/` tree.
    let entries = discover_rhei_entries(parent).ok()?;
    if !entries.iter().any(|candidate| candidate.file_name() == Some(name)) {
        return None;
    }
    let id = rhei_id_for_entry(&absolute).ok()?;
    Some((parent.to_path_buf(), id))
}
