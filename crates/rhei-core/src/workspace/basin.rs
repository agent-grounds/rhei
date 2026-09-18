//! The synthetic `basin` rhei: the project's home for tickets that belong to
//! no rhei yet.
//!
//! It has no authored index — its manifest is the project's, so a `basin/`
//! directory is a bag of task files and nothing else. That is the whole of its
//! difference from an ordinary Directory Workspace rhei, and the reason it is
//! loaded by its own function rather than by [`super::load_workspace`]: any
//! `*.md` under it is a task file, `index.rhei.md` is a mistake worth naming,
//! and `basin` is a reserved id whether or not the directory exists.

// §FS-rhei-panta.2 §AR-rhei-panta.1

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{Rhei, Structure};
use crate::parser::{self, ParseError};

use super::{collect_task_sources, nested_parse_error, Workspace, RHEI_INDEX_FILE};

pub const BASIN_RHEI_ID: &str = "basin";

pub(super) fn load_basin_rhei(
    dir: &Path,
    structure: &Structure,
    states: &str,
) -> parser::Result<Workspace> {
    let _guards =
        crate::root_access::for_input(dir).map_err(|err| ParseError::new(err.to_string(), None))?;
    let mut tasks = Vec::new();
    let mut task_sources = HashMap::new();
    for path in discover_basin_task_files(dir)? {
        // The basin's manifest is synthetic, so an authored index can never
        // load. Skipping it silently vanished its tickets behind a green
        // validation — what the basin exists to prevent. §AR-rhei-panta.1
        if path.file_name().and_then(|name| name.to_str()) == Some(RHEI_INDEX_FILE) {
            return Err(ParseError::new(
                format!(
                    "{}: the basin has no authored index — its manifest is synthetic, so this \
                     file would never load. Move its tickets into task files under {} (any \
                     `*.md` file works), or rename the directory to make it an ordinary rhei \
                     with its own id.",
                    path.display(),
                    dir.display()
                ),
                None,
            ));
        }
        let content = crate::source::read_to_string(&path).map_err(|e| {
            ParseError::new(format!("failed to read {}: {e}", path.display()), None)
        })?;
        let parsed = parser::parse_workspace_tasks_with_structure(&content, structure)
            .map_err(|e| nested_parse_error(e, &path))?;
        for task in &parsed {
            collect_task_sources(task, &path, &mut task_sources)?;
        }
        tasks.extend(parsed);
    }
    Ok(Workspace {
        root_guards: crate::root_access::for_input(dir)
            .map_err(|err| ParseError::new(err.to_string(), None))?,
        rhei: Rhei {
            title: "Basin".to_string(),
            states: states.to_string(),
            states_declared: false,
            structure: structure.clone(),
            metadata: None,
            content_sections: Vec::new(),
            tasks,
        },
        task_sources,
    })
}

pub(super) fn discover_basin_task_files(dir: &Path) -> parser::Result<Vec<PathBuf>> {
    fn is_hidden(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.'))
    }

    fn visit(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> parser::Result<()> {
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
                // Basin runtime artifacts are not authored basin task files. §FS-rhei-panta.2
                if path
                    .strip_prefix(root)
                    .ok()
                    .and_then(|relative| relative.components().next())
                    .is_some_and(|component| component.as_os_str() == "runtime")
                {
                    continue;
                }
                visit(root, &path, out)?;
            } else if file_type.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("md")
            {
                out.push(path);
            }
        }

        Ok(())
    }

    let mut files = Vec::new();
    if dir.is_dir() {
        visit(dir, dir, &mut files)?;
    }
    files.sort_by(|a, b| {
        let a_key = a.strip_prefix(dir).unwrap_or(a).to_string_lossy().replace('\\', "/");
        let b_key = b.strip_prefix(dir).unwrap_or(b).to_string_lossy().replace('\\', "/");
        a_key.cmp(&b_key)
    });
    Ok(files)
}

/// The `basin` id belongs to the synthetic catch-all rhei that a Panta
/// project's `basin/` directory feeds; a user rhei may not claim it.
/// §FS-rhei-panta.4
pub(super) fn basin_id_reserved_error(entry: &Path) -> ParseError {
    ParseError::new(
        format!(
            "`{BASIN_RHEI_ID}` is reserved: a Panta project's `{BASIN_RHEI_ID}/` directory \
             feeds the synthetic catch-all rhei, so {} cannot use that id. Rename the file \
             or directory to any other id",
            entry.display()
        ),
        None,
    )
}
