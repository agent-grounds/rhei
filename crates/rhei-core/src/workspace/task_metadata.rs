//! Folding the metadata blocks a workspace's task files authored into the
//! index's frontmatter.
//!
//! Its own module because two loaders need exactly one implementation of the
//! rule: [`super::load_workspace`], and the error-collecting loader
//! `rhei validate` uses. Splitting it in two is how a workspace would come to
//! merge under one command and refuse under another.

// §FS-rhei-plan-language.1.4 §AR-rhei-panta.2

use std::path::{Path, PathBuf};

use serde_yaml::Value as YamlValue;

use crate::ast::Metadata;
use crate::parser::{self, ParseError};

use super::qualify::{frontmatter_tasks, set_frontmatter_tasks};

/// One task file's authored frontmatter, as its loader hands it to the merge.
/// §FS-rhei-plan-language.1.4
#[derive(Debug, Clone)]
pub struct AuthoredTaskMetadata {
    /// The task file the block was read from — named by the overlap error.
    pub path: PathBuf,
    /// The block itself, already held to the rules one file can settle alone.
    pub metadata: Metadata,
}

/// Merge the task files' authored `metadata.tasks.<id>` entries into the
/// index's frontmatter, under the rhei-local ids the files spell.
///
/// The two spaces are **disjoint by key**: the index's entry and a task file's
/// entry for one task merge key by key, and the same key in both is a load
/// error naming both files and the key, so no reader has to learn a precedence
/// rule to predict which value wins. Naming the second file needs no provenance
/// map — a task file carries entries only for ids it defines and ids are unique
/// across `tasks/`, so every overlap is index-versus-exactly-one-task-file.
/// §FS-rhei-plan-language.1.4 §FS-rhei-validate.4.4
pub fn merge_authored_task_metadata(
    index_metadata: Option<Metadata>,
    index_path: &Path,
    authored: &[AuthoredTaskMetadata],
) -> parser::Result<Option<Metadata>> {
    let index_tasks = index_metadata.as_ref().and_then(frontmatter_tasks).unwrap_or_default();
    let mut merged = index_tasks.clone();
    let mut authored_any = false;

    for file in authored {
        let Some(entries) = frontmatter_tasks(&file.metadata) else {
            continue;
        };
        for (id, entry) in entries {
            match (index_tasks.get(&id), entry) {
                (Some(YamlValue::Mapping(from_index)), YamlValue::Mapping(from_file)) => {
                    let mut entry = from_index.clone();
                    for (key, value) in from_file {
                        if from_index.contains_key(&key) {
                            return Err(overlap_error(&id, &key, index_path, &file.path));
                        }
                        entry.insert(key, value);
                    }
                    merged.insert(id, YamlValue::Mapping(entry));
                }
                // One of the two spelled the task's metadata as something other
                // than a mapping, so there is nothing to merge key by key: the
                // whole entry is set twice.
                (Some(_), _) => return Err(entry_overlap_error(&id, index_path, &file.path)),
                (None, entry) => {
                    merged.insert(id, entry);
                }
            }
            authored_any = true;
        }
    }

    if !authored_any {
        return Ok(index_metadata);
    }
    let mut metadata = index_metadata.unwrap_or_default();
    set_frontmatter_tasks(&mut metadata, merged);
    Ok(Some(metadata))
}

/// Render a `metadata.tasks` key or a metadata key for a message: the id or key
/// as the file spells it, without YAML's quoting.
fn spelled(key: &YamlValue) -> String {
    match key {
        YamlValue::String(text) => text.clone(),
        other => serde_yaml::to_string(other).unwrap_or_default().trim().to_string(),
    }
}

/// The overlap error of §FS-rhei-validate.4.4 row 5.
fn overlap_error(id: &YamlValue, key: &YamlValue, index: &Path, task_file: &Path) -> ParseError {
    ParseError::new(
        format!(
            "metadata key `{}` for task `{}` is set in both {} and {}. A task file's metadata \
             and the index's entry for the same task are disjoint by key, so keep `{}` in one of \
             them.",
            spelled(key),
            spelled(id),
            index.display(),
            task_file.display(),
            spelled(key)
        ),
        None,
    )
}

/// The same refusal where one of the two entries is not a mapping, so no key
/// can be named. §FS-rhei-validate.4.4
fn entry_overlap_error(id: &YamlValue, index: &Path, task_file: &Path) -> ParseError {
    ParseError::new(
        format!(
            "metadata for task `{}` is set in both {} and {}, and at least one of the two is not \
             a mapping of keys. A task file's metadata and the index's entry for the same task \
             are disjoint by key: keep the entry in one of them.",
            spelled(id),
            index.display(),
            task_file.display()
        ),
        None,
    )
}
