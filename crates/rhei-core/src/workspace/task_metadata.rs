//! Folding the metadata blocks a workspace's task files authored into the
//! index's frontmatter.
//!
//! Its own module because two loaders need exactly one implementation of the
//! rule: [`super::load_workspace`], and the error-collecting loader
//! `rhei validate` uses. Splitting it in two is how a workspace would come to
//! merge under one command and refuse under another.

// §FS-rhei-plan-language.1.4 §AR-rhei-panta.2

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_yaml::Value as YamlValue;

use crate::ast::Metadata;
use crate::parser::{self, metadata_task_id, ParseError};

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
/// rule to predict which value wins.
///
/// Which entries are one task's is decided by [`metadata_task_id`] rather than
/// by the YAML scalar a file spells the id with, because the runtime writes a
/// multi-segment id into the index as a string while a bare `1.2:` authored in
/// a task file is a float: comparing the raw keys would read one task as two,
/// merge nothing, refuse nothing, and drop the index's entry — the runtime's own
/// counters included. §FS-rhei-plan-language.1.4 §FS-rhei-validate.4.4
pub fn merge_authored_task_metadata(
    index_metadata: Option<Metadata>,
    index_path: &Path,
    authored: &[AuthoredTaskMetadata],
) -> parser::Result<Option<Metadata>> {
    let index_tasks = index_metadata.as_ref().and_then(frontmatter_tasks).unwrap_or_default();
    let mut merged = index_tasks.clone();
    let mut held: HashMap<String, HeldEntry> = index_tasks
        .keys()
        .filter_map(|key| {
            metadata_task_id(key).map(|id| (id, HeldEntry::new(key.clone(), index_path)))
        })
        .collect();
    let mut authored_any = false;

    for file in authored {
        let Some(entries) = frontmatter_tasks(&file.metadata) else {
            continue;
        };
        for (id, entry) in entries {
            // The key a task already holds in the merged map, found by id
            // rather than by spelling, so one task stays one entry.
            let canonical = metadata_task_id(&id);
            let holder = match &canonical {
                Some(task_id) => held.get(task_id).cloned(),
                None => merged.contains_key(&id).then(|| HeldEntry::new(id.clone(), index_path)),
            };
            let standing = holder.as_ref().and_then(|holder| merged.get(&holder.key).cloned());
            match (standing, holder, entry) {
                (
                    Some(YamlValue::Mapping(standing)),
                    Some(holder),
                    YamlValue::Mapping(from_file),
                ) => {
                    let mut entry = standing.clone();
                    for (key, value) in from_file {
                        if standing.contains_key(&key) {
                            return Err(overlap_error(&id, &key, &holder.source, &file.path));
                        }
                        entry.insert(key, value);
                    }
                    merged.insert(holder.key, YamlValue::Mapping(entry));
                }
                // One of the two spelled the task's metadata as something other
                // than a mapping, so there is nothing to merge key by key: the
                // whole entry is set twice.
                (Some(_), Some(holder), _) => {
                    return Err(entry_overlap_error(&id, &holder.source, &file.path))
                }
                (_, _, entry) => {
                    if let Some(task_id) = canonical {
                        held.insert(task_id, HeldEntry::new(id.clone(), &file.path));
                    }
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

/// Where one task's entry stands in the merged map: the key it is filed under,
/// and the file that key came from, which an overlap error names.
#[derive(Debug, Clone)]
struct HeldEntry {
    key: YamlValue,
    source: PathBuf,
}

impl HeldEntry {
    fn new(key: YamlValue, source: &Path) -> Self {
        Self { key, source: source.to_path_buf() }
    }
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
fn overlap_error(id: &YamlValue, key: &YamlValue, standing: &Path, task_file: &Path) -> ParseError {
    ParseError::new(
        format!(
            "metadata key `{}` for task `{}` is set in both {} and {}. A task file's metadata \
             and the index's entry for the same task are disjoint by key, so keep `{}` in one of \
             them.",
            spelled(key),
            spelled(id),
            standing.display(),
            task_file.display(),
            spelled(key)
        ),
        None,
    )
}

/// The same refusal where one of the two entries is not a mapping, so no key
/// can be named. §FS-rhei-validate.4.4
fn entry_overlap_error(id: &YamlValue, standing: &Path, task_file: &Path) -> ParseError {
    ParseError::new(
        format!(
            "metadata for task `{}` is set in both {} and {}, and at least one of the two is not \
             a mapping of keys. A task file's metadata and the index's entry for the same task \
             are disjoint by key: keep the entry in one of them.",
            spelled(id),
            standing.display(),
            task_file.display()
        ),
        None,
    )
}
