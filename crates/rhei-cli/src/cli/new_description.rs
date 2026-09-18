// Where a description comes from, and what it is allowed to contain.
//
// Its own part because both questions are about the *argument*: reading it from
// a flag, a file, or standard input, and refusing text the plan language would
// read as structure rather than as prose.

// §FS-rhei-new.1.1 §FS-rhei-new.3.4

/// The metadata markers the plan language recognizes at the start of a line.
///
/// A description line opening with one of these stops being description: the
/// parser reads it as a field of the surrounding node, which is either an error
/// about metadata the author never wrote or a silently applied field.
// §FS-rhei-plan-language.2
const PLAN_METADATA_MARKERS: [&str; 9] = [
    "**State:**",
    "**States:**",
    "**Prior:**",
    "**Inherits:**",
    "**Provides:**",
    "**Consumes:**",
    "**Assignee:**",
    "**Model:**",
    "**Target:**",
];

/// The description body, from `--description` or `--description-file` (`-`
/// reads standard input), checked before it can reach a file.
// §FS-rhei-new.1.1
fn resolve_new_description(options: &NewOptions) -> MietteResult<Option<String>> {
    let (flag, body) = match (&options.description, &options.description_file) {
        (Some(description), _) => ("--description", description.clone()),
        (None, Some(path)) if path.as_os_str() == "-" => {
            let mut body = String::new();
            std::io::stdin().read_to_string(&mut body).map_err(|err| miette!(
help = "`--description-file -` reads the description from standard input; pipe it in, or pass a path.",
                "failed to read the description from standard input: {err}"))?;
            ("--description-file -", body)
        }
        (None, Some(path)) => {
            let body = fs::read_to_string(path).map_err(|err| description_file_report(path, err))?;
            ("--description-file", body)
        }
        (None, None) => return Ok(None),
    };
    reject_structural_description(&body, flag)?;
    Ok(Some(body))
}

/// Report a `--description-file` that could not be read.
///
/// The generic file report offers to `mkdir -p` the missing directory, which is
/// advice for a path being *written*; this one is being read, and the answer is
/// to check the path or pipe the text in instead.
// §FS-rhei-new.1.1 §FS-rhei-errors.1.2
fn description_file_report(path: &Path, err: std::io::Error) -> Report {
    let help = match err.kind() {
        std::io::ErrorKind::NotFound => format!(
            "no file there to read. Check the path, or pipe the text in with \
             `--description-file -`. Look with: ls {}",
            shell_quote(&path.parent().unwrap_or(Path::new(".")).display().to_string())
        ),
        std::io::ErrorKind::PermissionDenied => format!(
            "the current user cannot read that file. Inspect it with: ls -l {}",
            shell_quote(&path.display().to_string())
        ),
        _ => "the description is read from this path; check that it exists and is readable."
            .to_string(),
    };
    miette!(help = help, "failed to read the description from '{}': {err}", path.display())
}

/// Refuse a description line the plan language would read as structure.
///
/// The text is written into the plan verbatim, so an `### Task 9: …` line in a
/// description is not a formatting slip — it is a second ticket, carrying
/// whatever state the text supplied. Checked before the write and reported
/// against the flag that carried it: the offending text is an argument, and a
/// code frame pointing into a plan file the author never opened is not
/// something they can act on.
// §FS-rhei-new.3.4
fn reject_structural_description(description: &str, flag: &str) -> MietteResult<()> {
    let mut fence_opened_at: Option<usize> = None;
    for (index, raw) in description.lines().enumerate() {
        if raw.trim_start().starts_with("```") {
            fence_opened_at = match fence_opened_at {
                Some(_) => None,
                None => Some(index + 1),
            };
            continue;
        }
        // Fenced lines are content, not structure — the parser reads them that
        // way too, so they stay accepted exactly as written.
        if fence_opened_at.is_some() {
            continue;
        }
        let Some(what) = structural_description_line(raw) else {
            continue;
        };
        return Err(miette!(
            help = structural_description_help(),
            "line {} of {flag} would be read as plan structure rather than as description \
             ({what}):\n\n    {}\n\n`rhei new` writes the description into the plan as given, \
             so this line would author part of the plan instead of describing the ticket. \
             Nothing was written.",
            index + 1,
            raw.trim()
        ));
    }
    if let Some(line) = fence_opened_at {
        return Err(unbalanced_fence_report(flag, line));
    }
    Ok(())
}

/// Refuse a description whose ``` fences do not balance.
///
/// An odd number of fences is the ordinary shape of a pasted issue body, and it
/// is the most destructive thing a description can carry: the fence is written
/// verbatim, so every node *after* the insertion point becomes fenced content
/// and stops being a ticket. Named against the flag, like every other
/// description check — "your fence is unclosed" is something the author can act
/// on, where the whole-id-set guard behind the write is only the last line of
/// defence.
// §FS-rhei-new.3.4 §FS-rhei-new.5.1
fn unbalanced_fence_report(flag: &str, opened_at: usize) -> Report {
    miette!(
help = "close the fence with a matching ``` line, or drop the opening one. A stray fence is usually a pasted issue body that was cut short.",

        "the code fence opened on line {opened_at} of {flag} is never closed:\n\n`rhei new` \
         writes the description into the plan as given, so an open fence would swallow every \
         ticket after it — they would stop being plan nodes and become fenced text. Nothing \
         was written."
    )
}

/// The three ways to keep the line, all of which leave the author's words
/// intact. `rhei new` applies none of them itself: a create that quietly
/// rewrote an issue body pasted into `--description-file` would be worse than
/// one that refused it.
// §FS-rhei-new.3.4
fn structural_description_help() -> &'static str {
    "keep the line by fencing it in ```…```, writing it as bold text \
     (`**Design notes**`), or escaping the marker (`\\### Design notes`). Leading whitespace \
     does not help: the plan parser trims each line before reading it."
}

/// Name what the plan language would make of `line`, or `None` when it is
/// ordinary prose. Matched against the trimmed line, because that is what the
/// plan lexer matches against. §FS-rhei-plan-language.2
fn structural_description_line(line: &str) -> Option<&'static str> {
    let line = line.trim();
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&hashes) {
        let rest = &line[hashes..];
        if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            return Some("an ATX heading, which the plan language reads as a node, a chapter, \
                         or the rhei title");
        }
    }
    // A rhei description sits directly under `# Rhei: <title>`, where the
    // parser looks for frontmatter: `---` there authors `structure:`.
    // §FS-rhei-plan-language.1.1
    if line == "---" {
        return Some("the opening of a frontmatter block, which the plan language reads as \
                     the rhei's `structure:` and metadata");
    }
    PLAN_METADATA_MARKERS
        .iter()
        .any(|marker| line.starts_with(marker))
        .then_some("a metadata field of the node it lands in")
}
