//! The leading metadata block of a bare task file: splitting it off before the
//! node parser reads the rest, and checking what it may say.
//! §FS-rhei-plan-language.1.4 §FS-rhei-validate.4.4

use crate::ast::{Metadata, Task};
use crate::header::{find_leading_frontmatter, FrontmatterBlock};

use super::{ParseError, Result};

/// The leading `---` block of a bare task file, split off before the file is
/// wrapped in the synthetic single-file plan the node parser reads.
pub(super) struct LeadingFrontmatter {
    /// The lines between the delimiters, as `parse_frontmatter` wants them.
    pub(super) lines: Vec<String>,
    /// The opening `---`, 1-based in the task file.
    pub(super) open_line: usize,
    /// The first line *inside* the block, 1-based in the task file.
    pub(super) start_line: usize,
    /// How many of the task file's lines the block and the blanks above it
    /// occupy, so the remainder's first line is number `consumed + 1`.
    pub(super) consumed: usize,
}

/// Split an optional leading frontmatter block off a bare task file, returning
/// it and the remainder the node parser reads.
///
/// The block is parsed *before* the synthetic prefix is prepended rather than
/// merged into it: that prefix already carries a frontmatter block of its own
/// when the index declares `structure`, so merging would let a task file set a
/// plan-wide setting through the plan's own frontmatter position, and the
/// manifest parser refuses a `---` above the `# Rhei:` header. Splitting leaves
/// the prefix byte-for-byte what it was, which is what keeps
/// `adjust_workspace_task_error_line` correct.
/// §FS-rhei-plan-language.2 §FS-rhei-validate.4.4
pub(super) fn split_leading_frontmatter(
    input: &str,
) -> Result<(Option<LeadingFrontmatter>, String)> {
    // A leading mark would hide the opening `---`, and the block with it.
    // §FS-rhei-language-reference.1 §REQ-cross-platform.9
    let input = crate::source::without_byte_order_mark(input);
    let lines: Vec<&str> = input.lines().collect();
    let Some(FrontmatterBlock { open, close }) = find_leading_frontmatter(lines.iter().copied())
    else {
        return Ok((None, input.to_string()));
    };
    let Some(close) = close else {
        return Err(ParseError::new(
            "Unterminated YAML frontmatter: missing closing '---'",
            Some(open + 1),
        ));
    };
    let body = lines[open + 1..close].iter().map(|line| (*line).to_string()).collect();

    let consumed = close + 1;
    let mut remainder = lines[consumed..].join("\n");
    if input.ends_with('\n') && !remainder.is_empty() {
        remainder.push('\n');
    }
    let block =
        LeadingFrontmatter { lines: body, open_line: open + 1, start_line: open + 2, consumed };
    Ok((Some(block), remainder))
}

/// The refusal for frontmatter in a file under `basin/`. §FS-rhei-validate.4.4
pub(super) fn basin_frontmatter_error(block: &LeadingFrontmatter) -> ParseError {
    ParseError::new(
        "a basin ticket file carries no frontmatter — the basin's manifest is synthetic, so \
         basin ticket metadata lives in `index.panta.md` under project-qualified ids. Move the \
         block there, or rename the directory to make it an ordinary rhei with its own id.",
        Some(block.open_line),
    )
}

/// The line a top-level key of the block sits on, for the code frame.
///
/// Unindented and followed by `:`, which is what a top-level key of a YAML
/// mapping looks like; `None` when the block spells it some other way, and then
/// the error carries the block's own opening line instead.
fn top_level_key_line(block: &LeadingFrontmatter, key: &str) -> Option<usize> {
    block
        .lines
        .iter()
        .position(|line| line.starts_with(key) && line[key.len()..].trim_start().starts_with(':'))
        .map(|offset| block.start_line + offset)
}

/// Every node id a task file defines, as that file spells it — the ids its own
/// `metadata.tasks` entries may name. §FS-rhei-plan-language.1.4
fn defined_ids(tasks: &[Task]) -> Vec<String> {
    fn visit(task: &Task, out: &mut Vec<String>) {
        out.push(task.id.to_string());
        for child in &task.children {
            visit(child, out);
        }
    }
    let mut ids = Vec::new();
    for task in tasks {
        visit(task, &mut ids);
    }
    ids
}

/// A `metadata.tasks` key as a task id, for a key whose authored text is no
/// longer in hand: the ids a numbered ticket authors are YAML numbers, and `3`
/// names the same ticket as `"3"`.
///
/// The one rule for what a `metadata.tasks` key identifies, so the parser's
/// check and the merge cannot come to disagree about it — a task with two keys
/// in the merged map is a task whose entries silently replace one another.
///
/// A parsed number cannot always say how it was written, so this is a fallback
/// rather than the rule: a task file's keys are re-read from their own text by
/// [`normalize_task_metadata_ids`] before anything asks what they name, and
/// what reaches here is `index.rhei.md`'s own keys, which the runtime writes.
/// §FS-rhei-plan-language.1.4
pub(crate) fn metadata_task_id(key: &serde_yaml::Value) -> Option<String> {
    match key {
        serde_yaml::Value::String(id) => Some(id.clone()),
        serde_yaml::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// A task file's `metadata.tasks` keys as the text the file spells them with,
/// in the order the block writes them.
///
/// YAML resolves a plain scalar key before the mapping is built, and what it
/// resolves to cannot always say how it was written: `1.10` and `1.1` are one
/// `f64`, so an id recovered from the parsed number names task `1.1` whatever
/// the author wrote, and `true:` resolves to a scalar no id can be read out of
/// at all. Deserializing the same block with `String` keys asks the parser for
/// each key's own text instead, which is exact for every scalar a key can be.
///
/// `None` where the block is not shaped like one carrying entries — a key that
/// is not a scalar, a `metadata` that is not a mapping — and then the keys stay
/// as they were read and the checks below name what is wrong with them.
/// §FS-rhei-plan-language.1.4
fn spelled_task_metadata_ids(lines: &[String]) -> Option<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct Block {
        metadata: Option<Section>,
    }

    #[derive(serde::Deserialize)]
    struct Section {
        tasks: Option<SpelledIds>,
    }

    /// Every key of the `tasks` mapping, duplicates kept: two spellings of one
    /// id are two keys here and one entry in the parsed mapping, and the count
    /// is what says so.
    struct SpelledIds(Vec<String>);

    impl<'de> serde::Deserialize<'de> for SpelledIds {
        fn deserialize<D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> std::result::Result<Self, D::Error> {
            struct Keys;

            impl<'de> serde::de::Visitor<'de> for Keys {
                type Value = SpelledIds;

                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a mapping of task id to that task's metadata")
                }

                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> std::result::Result<SpelledIds, A::Error> {
                    let mut ids = Vec::new();
                    while let Some(id) = map.next_key::<String>()? {
                        map.next_value::<serde::de::IgnoredAny>()?;
                        ids.push(id);
                    }
                    Ok(SpelledIds(ids))
                }
            }

            deserializer.deserialize_map(Keys)
        }
    }

    serde_yaml::from_str::<Block>(&lines.join("\n")).ok()?.metadata?.tasks.map(|ids| ids.0)
}

/// Re-key a task file's `metadata.tasks` entries under the ids the file spells,
/// so the merge, the check below and the project qualification all read the
/// task the author named rather than the one its scalar rounds to.
///
/// Returns the error of §FS-rhei-validate.4.4 row 3 where two of the file's own
/// keys spell one id: one task with two entries is one entry silently
/// discarded, which is the outcome that table exists to rule out. The re-keying
/// still happens, because the load is refused either way and every later check
/// reads better ids for it. §FS-rhei-plan-language.1.4
pub(super) fn normalize_task_metadata_ids(
    metadata: &mut Metadata,
    block: &LeadingFrontmatter,
) -> Option<ParseError> {
    let metadata_key = serde_yaml::Value::String("metadata".to_string());
    let tasks_key = serde_yaml::Value::String("tasks".to_string());
    let Some(serde_yaml::Value::Mapping(mut section)) = metadata.get(&metadata_key).cloned() else {
        return None;
    };
    let Some(serde_yaml::Value::Mapping(entries)) = section.get(&tasks_key).cloned() else {
        return None;
    };
    let spelled = spelled_task_metadata_ids(&block.lines)?;
    if spelled.len() != entries.len() {
        return None;
    }

    let mut error = None;
    let mut renamed = Metadata::new();
    for (id, (_, value)) in spelled.into_iter().zip(entries) {
        let key = serde_yaml::Value::String(id.clone());
        if renamed.contains_key(&key) && error.is_none() {
            error = Some(ParseError::new(
                format!(
                    "`metadata.tasks` carries two entries for task `{id}` in this file. A key \
                     names the task its own text spells, so two spellings of `{id}` are one task \
                     rather than two: keep one entry for it."
                ),
                top_level_key_line(block, "metadata").or(Some(block.open_line)),
            ));
        }
        renamed.insert(key, value);
    }

    section.insert(tasks_key, serde_yaml::Value::Mapping(renamed));
    metadata.insert(metadata_key, serde_yaml::Value::Mapping(section));
    error
}

/// A frontmatter key as a message names it: the key itself, without YAML's
/// quoting, at whatever depth the block spells it.
fn metadata_key_name(key: &serde_yaml::Value) -> String {
    match key {
        serde_yaml::Value::String(name) => name.clone(),
        other => serde_yaml::to_string(other).unwrap_or_default().trim().to_string(),
    }
}

/// Hold a task file's frontmatter block to the two rules one file can settle on
/// its own: metadata only, and its own tasks only.
///
/// `tasks` is `None` when the body did not parse, and then the entries name no
/// ids this can check — the body error is the one worth reporting.
/// §FS-rhei-plan-language.1.4 §FS-rhei-validate.4.4
pub(super) fn check_task_metadata_block(
    metadata: &Metadata,
    block: &LeadingFrontmatter,
    tasks: Option<&[Task]>,
) -> Vec<ParseError> {
    let mut errors = Vec::new();

    for (key, value) in metadata {
        let name = metadata_key_name(key);
        if name == "metadata" {
            match value {
                // A key under `metadata` other than `tasks` — `taks` for
                // `tasks` — is the same mistake one level down, and is named
                // rather than dropped. §FS-rhei-validate.4.4
                serde_yaml::Value::Mapping(section) => {
                    errors.extend(section.keys().filter_map(|key| {
                        let nested = metadata_key_name(key);
                        (nested != "tasks").then(|| {
                            ParseError::new(
                                format!(
                                    "`metadata.{nested}` is not allowed in a workspace task \
                                     file's frontmatter: the one permitted key under \
                                     `metadata` is `tasks`, carrying `metadata.tasks.<id>` \
                                     entries for the tasks this file defines."
                                ),
                                top_level_key_line(block, &name).or(Some(block.open_line)),
                            )
                        })
                    }));
                }
                _ => errors.push(ParseError::new(
                    "`metadata` in a workspace task file's frontmatter must be a mapping \
                     carrying `tasks.<id>` entries for the tasks this file defines.",
                    top_level_key_line(block, &name).or(Some(block.open_line)),
                )),
            }
            continue;
        }
        let line = top_level_key_line(block, &name).or(Some(block.open_line));
        // `structure` earns its own wording because it is the one plan-wide key
        // an author plausibly writes into the wrong file. §FS-rhei-validate.4.4
        errors.push(if name == "structure" {
            ParseError::new(
                "`structure` is a plan-wide setting and belongs in `index.rhei.md`, which \
                 declares it once for every task file under `tasks/`. A workspace task file's \
                 frontmatter carries only `metadata.tasks.<id>` for the tasks it defines.",
                line,
            )
        } else {
            ParseError::new(
                format!(
                    "`{name}` is not allowed in a workspace task file's frontmatter: its one \
                     permitted top-level key is `metadata`, carrying `metadata.tasks.<id>` \
                     entries for the tasks this file defines. A plan-wide setting belongs in \
                     `index.rhei.md`."
                ),
                line,
            )
        });
    }

    let Some(tasks) = tasks else {
        return errors;
    };
    let Some(entries) = metadata
        .get(serde_yaml::Value::String("metadata".to_string()))
        .and_then(|section| section.get("tasks"))
    else {
        return errors;
    };
    let serde_yaml::Value::Mapping(entries) = entries else {
        errors.push(ParseError::new(
            "`metadata.tasks` in a workspace task file's frontmatter must be a mapping of task \
             id to that task's metadata.",
            top_level_key_line(block, "metadata").or(Some(block.open_line)),
        ));
        return errors;
    };

    let defined = defined_ids(tasks);
    for key in entries.keys() {
        // A key no id can be read out of — `true:`, a mapping — names no task
        // either, and stepping past it is how a block is accepted and then
        // discarded. §FS-rhei-validate.4.4
        let id = metadata_task_id(key).unwrap_or_else(|| metadata_key_name(key));
        if defined.iter().any(|defined| *defined == id) {
            continue;
        }
        errors.push(ParseError::new(
            format!(
                "`metadata.tasks.{id}` names no task defined in this file. Move the entry to \
                 the file that defines `{id}`, or to `index.rhei.md`."
            ),
            top_level_key_line(block, "metadata").or(Some(block.open_line)),
        ));
    }

    errors
}
