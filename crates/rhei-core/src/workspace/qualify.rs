//! What a rhei id may be, and what the merge does to a rhei's ids.
//!
//! A rhei authors its tickets with local ids (`3`, `api.cache`); the project
//! sees them qualified by the owning rhei (`auth.3`). Qualification touches
//! every id a ticket carries — its own, its `**Prior:**`, its `**Consumes:**` —
//! and the frontmatter metadata keyed by those ids, and it must leave an id
//! that already names another rhei alone.
//!
//! Its own module because these are pure rewrites over ids and metadata: they
//! read no file and decide nothing about what to load.

// §AR-rhei-panta.3 §FS-rhei-panta.5

use std::collections::HashSet;
use std::path::Path;

use crate::ast::{Metadata, Structure, Task, TaskId, TaskIdSegment};
use crate::parser::{self, ParseError};

pub(super) fn rhei_id_for_entry(path: &Path) -> parser::Result<String> {
    if path.is_dir() {
        // Spelling must not change the id, and `.`, `./`, and a trailing `..`
        // carry no name: resolve those only, so a symlinked workspace keeps the
        // id it has today. §FS-rhei-panta.6 §AR-rhei-panta.3
        return dir_name(path)
            .or_else(|| std::fs::canonicalize(path).ok().as_deref().and_then(dir_name))
            .ok_or_else(|| nameless_dir_error(path));
    }

    let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
        ParseError::new(format!("invalid rhei filename {}", path.display()), None)
    })?;
    // §AR-rhei-panta.3: the rhei id is the file stem, so a single-file rhei
    // must carry the `.rhei.md` suffix for its id to exist.
    let Some(stem) = name.strip_suffix(".rhei.md") else {
        return Err(ParseError::new(
            format!(
                "'{}' is not a rhei plan file: a single-file rhei must be named `<id>.rhei.md`, \
                 because the id every ticket is prefixed with comes from the file stem",
                path.display()
            ),
            None,
        ));
    };
    Ok(stem.to_string())
}

/// The trailing component of `path` as a rhei id candidate.
///
/// `None` when the path carries no name of its own — `.`, `./`, anything ending
/// in `..`, the filesystem root — or when that name is not UTF-8.
fn dir_name(path: &Path) -> Option<String> {
    path.file_name().and_then(|name| name.to_str()).map(ToOwned::to_owned)
}

/// The error for a directory that resolves to no usable name: the filesystem
/// root, a name that is not UTF-8, or a path that would not canonicalize.
///
/// It names path resolution as what failed, because everything the reader
/// authored is fine and it is the path that carries no id — the plan-authoring
/// help would send them to edit a valid file. §FS-rhei-panta.6
fn nameless_dir_error(path: &Path) -> ParseError {
    ParseError::new(
        format!(
            "cannot derive a rhei id from {}: the path resolves to no usable directory name",
            path.display()
        ),
        None,
    )
}

pub(super) fn validate_rhei_id(id: &str, path: &Path) -> parser::Result<()> {
    let valid = id.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if valid {
        return Ok(());
    }
    let rename_hint = match suggest_rhei_id(id) {
        Some(suggestion) if path.is_dir() => {
            format!(" Rename the directory to `{suggestion}/` to fix the id.")
        }
        Some(suggestion) => {
            format!(" Rename the file to `{suggestion}.rhei.md` to fix the id.")
        }
        None => String::new(),
    };
    Err(ParseError::new(
        format!(
            "rhei id '{id}' derived from {} is not valid: a rhei id must start with a \
             letter and contain only letters, digits, `_`, or `-`, because it prefixes \
             every ticket id in the project.{rename_hint}",
            path.display()
        ),
        None,
    ))
}

/// Best-effort legal rhei id for a rename suggestion: invalid characters
/// become `-`, and anything before the first letter is dropped.
pub(super) fn suggest_rhei_id(id: &str) -> Option<String> {
    let mut out = String::new();
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String =
        out.trim_matches('-').chars().skip_while(|ch| !ch.is_ascii_alphabetic()).collect();
    (!out.is_empty()).then_some(out)
}

pub(super) fn merge_structure(into: &mut Structure, from: &Structure) {
    // Keep authored max-level constraints while merging per-rhei node kinds. §FS-rhei-states.9.3
    into.max_levels = into.max_levels.max(from.max_levels);
    for kind in &from.node_kinds {
        if !into.node_kinds.iter().any(|existing| existing.eq_ignore_ascii_case(kind)) {
            into.node_kinds.push(kind.clone());
        }
    }
}

pub(super) fn collect_task_ids(tasks: &[Task]) -> HashSet<TaskId> {
    fn visit(task: &Task, out: &mut HashSet<TaskId>) {
        out.insert(task.id.clone());
        for child in &task.children {
            visit(child, out);
        }
    }

    let mut ids = HashSet::new();
    for task in tasks {
        visit(task, &mut ids);
    }
    ids
}

pub(super) fn qualify_tasks(tasks: &mut [Task], rhei_id: &str, local_ids: &HashSet<TaskId>) {
    for task in tasks {
        qualify_task(task, rhei_id, local_ids);
    }
}

pub(super) fn qualify_task(task: &mut Task, rhei_id: &str, local_ids: &HashSet<TaskId>) {
    task.id = qualify_local_id(&task.id, rhei_id);
    task.profile_depth_offset = task.profile_depth_offset.saturating_add(1);
    for prior in &mut task.prior {
        // A dotted `<name>.<rest>` naming no local ticket is a cross-rhei
        // reference, kept as authored so a dangling one is never reported under
        // an id nobody wrote. §AR-rhei-panta.3
        if local_ids.contains(prior) || !is_cross_rhei_reference(prior) {
            *prior = qualify_local_id(prior, rhei_id);
        }
    }
    // A consumed export names a task, so it qualifies exactly as a prior does.
    // §FS-rhei-plan-language.3.12
    for consumed in &mut task.consumes {
        if local_ids.contains(&consumed.task) || !is_cross_rhei_reference(&consumed.task) {
            consumed.task = qualify_local_id(&consumed.task, rhei_id);
        }
    }
    // Export exclusions use the same local-first qualification rule as
    // consumed exports. §FS-rhei-plan-language.3.13
    for exclusion in &mut task.excludes {
        if let crate::ast::TaskExclusion::Export(export) = exclusion {
            if local_ids.contains(&export.task) || !is_cross_rhei_reference(&export.task) {
                export.task = qualify_local_id(&export.task, rhei_id);
            }
        }
    }
    for child in &mut task.children {
        qualify_task(child, rhei_id, local_ids);
    }
}

pub(super) fn qualify_local_id(id: &TaskId, rhei_id: &str) -> TaskId {
    let mut segments = Vec::with_capacity(id.segments.len() + 1);
    segments.push(TaskIdSegment::Named(rhei_id.to_string()));
    segments.extend(id.segments.clone());
    TaskId::from_segments(segments)
}

/// Whether `id` has the shape of a reference into another rhei: a dotted id
/// whose leading segment is a name rather than a number.
///
/// The leading segment is *not* checked against the project's rhei ids. A typo'd
/// rhei name is shaped like a cross-rhei reference and reads like one to the
/// author, so it stays as written and validation explains it. Prefixing it with
/// the citing rhei instead would report an id that appears in no file.
// §AR-rhei-panta.3: a dotted, name-led prior is kept as authored.
pub(super) fn is_cross_rhei_reference(id: &TaskId) -> bool {
    id.segments.len() > 1 && matches!(id.segments.first(), Some(TaskIdSegment::Named(_)))
}

/// Runtime task metadata lives at `metadata.tasks` inside the frontmatter
/// root mapping. Returns the `tasks` mapping, if present.
pub(super) fn frontmatter_tasks(metadata: &Metadata) -> Option<Metadata> {
    let section = metadata.get(serde_yaml::Value::String("metadata".to_string()))?;
    match section.get("tasks") {
        Some(serde_yaml::Value::Mapping(tasks)) => Some(tasks.clone()),
        _ => None,
    }
}

/// Store a `tasks` mapping back at `metadata.tasks` in the frontmatter root.
pub(super) fn set_frontmatter_tasks(metadata: &mut Metadata, tasks: Metadata) {
    let metadata_key = serde_yaml::Value::String("metadata".to_string());
    let mut section = match metadata.get(&metadata_key).cloned() {
        Some(serde_yaml::Value::Mapping(section)) => section,
        _ => Metadata::new(),
    };
    section
        .insert(serde_yaml::Value::String("tasks".to_string()), serde_yaml::Value::Mapping(tasks));
    metadata.insert(metadata_key, serde_yaml::Value::Mapping(section));
}

/// Merge one rhei's qualified `metadata.tasks` metadata into the project metadata.
pub(super) fn merge_task_metadata(project: &mut Option<Metadata>, child: Option<Metadata>) {
    let Some(child) = child else { return };
    let Some(child_tasks) = frontmatter_tasks(&child) else { return };
    if child_tasks.is_empty() {
        return;
    }
    let project = project.get_or_insert_with(Metadata::new);
    let mut merged = frontmatter_tasks(project).unwrap_or_default();
    for (key, value) in child_tasks {
        merged.insert(key, value);
    }
    set_frontmatter_tasks(project, merged);
}

/// Re-key a rhei's frontmatter `metadata.tasks` entries under project-qualified
/// ids so merged-graph reads resolve; write-back stays rhei-local. §AR-rhei-panta.2
pub(super) fn qualify_task_metadata(metadata: Option<Metadata>, rhei_id: &str) -> Option<Metadata> {
    let mut metadata = metadata?;
    if let Some(tasks) = frontmatter_tasks(&metadata) {
        let mut qualified = Metadata::new();
        for (key, value) in tasks {
            let local = match &key {
                serde_yaml::Value::String(s) => s.clone(),
                serde_yaml::Value::Number(n) => n.to_string(),
                _ => {
                    qualified.insert(key, value);
                    continue;
                }
            };
            qualified.insert(serde_yaml::Value::String(format!("{rhei_id}.{local}")), value);
        }
        set_frontmatter_tasks(&mut metadata, qualified);
    }
    Some(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory named by a path with no name of its own still has an id:
    /// `.` is the invocation directory, spelled. §FS-rhei-panta.6
    #[test]
    fn current_directory_derives_the_resolved_directory_name() {
        let expected = std::env::current_dir()
            .and_then(std::fs::canonicalize)
            .expect("the invocation directory should resolve");
        let expected = expected.file_name().expect("it should have a name").to_str().unwrap();

        assert_eq!(rhei_id_for_entry(Path::new(".")).unwrap(), expected);
        assert_eq!(rhei_id_for_entry(Path::new("./")).unwrap(), expected);
    }

    /// The one directory that resolves to no name is the filesystem root, and
    /// its error blames the path rather than the plan. §FS-rhei-panta.6
    #[test]
    fn a_path_resolving_to_no_name_blames_resolution() {
        let root = if cfg!(windows) { Path::new("C:\\") } else { Path::new("/") };
        let err = rhei_id_for_entry(root).expect_err("the root names no rhei");
        assert!(
            err.message.contains("resolves to no usable directory name"),
            "the message should name resolution; got: {}",
            err.message
        );
    }
}
