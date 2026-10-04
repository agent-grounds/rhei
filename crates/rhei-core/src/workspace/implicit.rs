//! A bare rhei — a single `.rhei.md` file or a Directory Workspace with no
//! `index.panta.md` above it — loaded as the single rhei of an implicit Panta,
//! so every command walks the same graph shape whether or not a project exists.
//!
//! Its own module because the explicit project's loader discovers, skips and
//! merges many rheis, while this wraps exactly one and has no manifest to read.
// §AR-rhei-panta.2 §AR-rhei-panta.3

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::Rhei;
use crate::parser::{self, ParseError};

use super::panta::{load_rhei_entry, rhei_execution_root, rhei_id_for_path, rhei_plan_file};
use super::qualify::{collect_task_ids, qualify_task_metadata, qualify_tasks};
use super::{
    collect_task_roots, collect_task_sources, source_for_task, workspace_dir, PantaProject,
    Workspace,
};

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
    // The implicit Panta has no manifest: the single rhei's machine resolves
    // from its own root like any member's. §AR-rhei-panta.2
    Ok(PantaProject {
        root_guards: loaded.root_guards,
        rhei,
        task_sources,
        task_roots,
        content_section_roots,
        rhei_ids,
        rhei_roots,
        rhei_titles,
        rhei_plans,
        unloadable: Vec::new(),
    })
}
