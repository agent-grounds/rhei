//! Directory Workspace loader for multi-file Rhei plans.
//!
//! A Directory Workspace consists of:
//! - `index.rhei.md`: root configuration with title, states, and content sections.
//! - `tasks/`: a directory containing `.md` files, each with one or more task definitions.
//!
//! All tasks are merged into a single global task graph. Task IDs must be
//! unique across the entire `tasks/` directory.
//!
//! That is one rhei. A **Panta project** is many of them under one directory,
//! discovered and merged into a single graph with every id qualified by its
//! owning rhei; that is a different job with different failure modes, and it
//! lives in [`panta`]. What the merge does to a rhei's ids, its metadata, and
//! its structure — and what a rhei id is allowed to be — lives in [`qualify`];
//! the one rhei a project synthesizes rather than discovers lives in [`basin`].

// §AR-rhei-panta

mod basin;
mod panta;
mod qualify;

pub use basin::BASIN_RHEI_ID;
pub use panta::{
    discover_rhei_entries, implicit_panta_from_file_rhei, is_panta_project, load_implicit_panta,
    load_panta_project, load_panta_project_lenient, load_panta_project_with_member, panta_member,
    panta_project_dir, rhei_id_for_path, rhei_plan_file, wrap_rhei_as_implicit_panta, PantaProject,
    PANTA_INDEX_FILE,
};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{Rhei, Task, TaskId};
use crate::parser::{self, ParseError};

/// The index document of a Directory Workspace rhei — the plan a reader
/// opens when the execution root is a directory. §FS-rhei-memory.3.4
pub const RHEI_INDEX_FILE: &str = "index.rhei.md";

/// A loaded directory workspace: the merged plan plus a map from each task ID
/// to the file it was parsed from (needed for targeted file rewrites during
/// transitions).
#[derive(Debug)]
pub struct Workspace {
    pub rhei: Rhei,
    /// Maps task ID (as string) → the file path that defines it.
    pub task_sources: HashMap<String, PathBuf>,
}

/// Re-raise a parse error from a nested document, recording which file its
/// line belongs to.
///
/// The path travels as data rather than as a message prefix so diagnostics can
/// still open the file and render a code frame. Flattening it into the message
/// cost every nested error its line number and source excerpt — exactly the
/// errors a project author hits first, since `rhei init` puts their plans one
/// level down.
fn nested_parse_error(err: ParseError, path: &Path) -> ParseError {
    // An error that already names its origin keeps it: the innermost file is
    // the one holding the line.
    if err.file.is_some() {
        return err;
    }
    err.in_file(path)
}

/// Returns `true` if `path` is a directory workspace
/// (a directory containing `index.rhei.md`).
pub fn is_workspace(path: &Path) -> bool {
    path.is_dir() && path.join(RHEI_INDEX_FILE).is_file()
}

/// The directory a plan path lives in, as a path that names a directory.
///
/// `Path::parent` answers `Some("")` for a bare relative filename —
/// `plan.rhei.md` typed at the shell — and an empty path names nothing. Joins
/// onto it keep working by accident (`"".join("runtime")` is `runtime`), so the
/// emptiness surfaces only where the directory itself is printed or exported:
/// a blank `RHEI_ROOT`, a blank execution root in the memory map. `.` is the
/// same directory, spelled.
// §FS-rhei-memory.3.4
pub fn plan_parent_dir(path: &Path) -> &Path {
    path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."))
}

/// Resolve the workspace directory for `path`, accepting either:
/// - a workspace directory (containing `index.rhei.md`), or
/// - the `index.rhei.md` file itself, when its parent directory contains
///   a `tasks/` subdirectory.
///
/// Callers that need the workspace root regardless of which form the user
/// supplied should prefer this over `is_workspace`.
pub fn workspace_dir(path: &Path) -> Option<PathBuf> {
    if is_workspace(path) {
        return Some(path.to_path_buf());
    }
    if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(RHEI_INDEX_FILE) {
        let parent = plan_parent_dir(path);
        if parent.join("tasks").is_dir() {
            return Some(parent.to_path_buf());
        }
    }
    None
}

/// Discover workspace task files recursively in deterministic plan order.
pub fn discover_task_files(tasks_dir: &Path) -> parser::Result<Vec<PathBuf>> {
    fn is_hidden(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.'))
    }

    fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> parser::Result<()> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| ParseError::new(format!("failed to read {}: {e}", dir.display()), None))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", dir.display()), None)
            })?;
            let path = entry.path();
            if is_hidden(&path) {
                continue;
            }
            let file_type = entry.file_type().map_err(|e| {
                ParseError::new(format!("failed to inspect {}: {e}", path.display()), None)
            })?;
            if file_type.is_dir() {
                visit(&path, out)?;
            } else if file_type.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("md")
            {
                out.push(path);
            }
        }

        Ok(())
    }

    let mut files = Vec::new();
    if tasks_dir.is_dir() {
        visit(tasks_dir, &mut files)?;
    }
    files.sort_by(|a, b| {
        let a_key = a.strip_prefix(tasks_dir).unwrap_or(a).to_string_lossy().replace('\\', "/");
        let b_key = b.strip_prefix(tasks_dir).unwrap_or(b).to_string_lossy().replace('\\', "/");
        a_key.cmp(&b_key)
    });
    Ok(files)
}

fn source_for_task(sources: &HashMap<String, PathBuf>, task: &Task) -> parser::Result<PathBuf> {
    let local = TaskId::from_segments(task.id.segments.iter().skip(1).cloned().collect());
    sources.get(&local.to_string()).cloned().ok_or_else(|| {
        // The rhei loader always records a source for every task it parses, so a
        // miss here means an internal qualification/source-map inconsistency, not
        // a user error. Fail loudly instead of pointing rewrites at an empty path.
        ParseError::new(
            format!(
                "internal: no source file recorded for task '{}' (rhei-local id '{}')",
                task.id, local
            ),
            None,
        )
    })
}

fn collect_task_roots(
    task: &Task,
    root: &Path,
    task_roots: &mut HashMap<String, PathBuf>,
) -> parser::Result<()> {
    let id = task.id.to_string();
    task_roots.insert(id, root.to_path_buf());
    for child in &task.children {
        collect_task_roots(child, root, task_roots)?;
    }
    Ok(())
}

/// Load a directory workspace, merging all task files into a single plan.
///
/// Reads `index.rhei.md` for plan metadata, then discovers and parses every
/// `.md` file inside the `tasks/` subdirectory. Reports duplicate task IDs
/// across files and missing structure.
pub fn load_workspace(dir: &Path) -> parser::Result<Workspace> {
    let index_path = dir.join(RHEI_INDEX_FILE);
    let index_content = crate::source::read_to_string(&index_path).map_err(|e| {
        ParseError::new(format!("failed to read {}: {e}", index_path.display()), None)
    })?;

    let index = parser::parse_workspace_index(&index_content)
        .map_err(|e| nested_parse_error(e, &index_path))?;

    let tasks_dir = dir.join("tasks");
    let mut all_tasks: Vec<Task> = Vec::new();
    let mut task_sources: HashMap<String, PathBuf> = HashMap::new();

    if tasks_dir.is_dir() {
        for path in discover_task_files(&tasks_dir)? {
            let content = crate::source::read_to_string(&path).map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", path.display()), None)
            })?;

            let tasks = parser::parse_workspace_tasks_with_structure(&content, &index.structure)
                .map_err(|e| nested_parse_error(e, &path))?;

            for task in &tasks {
                collect_task_sources(task, &path, &mut task_sources)?;
            }

            all_tasks.extend(tasks);
        }
    }

    // No task files is a valid, empty rhei. Failing here let one freshly
    // created directory break loading for every sibling rhei; `rhei validate`
    // warns instead. §FS-rhei-plan-language.1.2

    Ok(Workspace {
        rhei: Rhei {
            title: index.title,
            states: index.states,
            states_declared: index.states_declared,
            structure: index.structure,
            metadata: index.metadata,
            content_sections: index.content_sections,
            tasks: all_tasks,
        },
        task_sources,
    })
}

/// Line of the *last* heading in `path` declaring ticket `id_str`.
///
/// That is the redeclaration in both shapes this serves: the second of two
/// headings when a single file repeats an id, and the colliding heading in the
/// newly loaded file when two files share one. Runs only on the error path, so
/// re-reading the file costs nothing in the common case; a file that no longer
/// reads simply yields no line rather than masking the duplicate itself.
fn duplicate_heading_line(path: &Path, id_str: &str) -> Option<usize> {
    let source = crate::source::read_to_string(path).ok()?;
    let mut found = None;
    for (index, line) in source.lines().enumerate() {
        let rest = line.trim_start_matches('#');
        if rest.len() == line.len() {
            continue;
        }
        // `<kind> <id>:` — the id sits between the kind keyword and the colon.
        let Some((head, _)) = rest.split_once(':') else {
            continue;
        };
        if head.split_whitespace().nth(1) == Some(id_str) {
            found = Some(index + 1);
        }
    }
    found
}

fn collect_task_sources(
    task: &Task,
    path: &Path,
    task_sources: &mut HashMap<String, PathBuf>,
) -> parser::Result<()> {
    let id_str = task.id.to_string();
    if let Some(existing) = task_sources.get(&id_str) {
        // Two tickets in one file read as "defined in both X and X" if the
        // paths are printed unconditionally; say which case it is, and point
        // at the offending heading so the fix is a jump away.
        let message = if existing == path {
            format!("duplicate task ID '{}': declared twice in {}", id_str, path.display())
        } else {
            format!(
                "duplicate task ID '{}': defined in both {} and {}",
                id_str,
                existing.display(),
                path.display()
            )
        };
        return Err(ParseError::new(message, duplicate_heading_line(path, &id_str)).in_file(path));
    }
    task_sources.insert(id_str, path.to_path_buf());

    for child in &task.children {
        collect_task_sources(child, path, task_sources)?;
    }

    Ok(())
}
