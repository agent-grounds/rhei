//! Guard: no built-in template may print a `rhei transition` invocation that
//! the CLI would reject. `--from` is the compare-and-swap guard the command is
//! built on and is a required argument, so a template that shows an invocation
//! without it hands the reader a command that always exits 2 (#117).
//!
//! §FS-rhei-transition-cmd.2: the options table marks `--from <STATE>` as
//! required, and §FS-rhei-transition-cmd says why — only the caller whose
//! expected `--from` matches the task's actual state wins the race.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// Every run of whitespace in a joined line becomes one space, so a command
/// reads the same however the source indented it (a YAML block scalar indents
/// every line of the prompt it carries).
fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The lines one invocation may occupy. A line ending in a backslash is a shell
/// continuation and takes the line under it, which is how every template README
/// writes a command too long to fit on one. Outside a fence a line leaving an
/// inline code span open likewise takes the lines that close it, which is how
/// prose wraps a command it shows; inside a fence there are no code spans, only
/// those continuations, so an unmatched backtick joins nothing there. A blank
/// line ends whatever is pending, because a code span cannot cross one.
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut pending = String::new();
    let mut fenced = false;
    let mut flush = |pending: &mut String| {
        if !pending.is_empty() {
            lines.push(normalize_whitespace(pending));
            pending.clear();
        }
    };
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            flush(&mut pending);
            fenced = !fenced;
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut pending);
            continue;
        }
        if !pending.is_empty() {
            pending.push(' ');
        }
        let continued = line.trim_end().ends_with('\\');
        pending.push_str(line.trim_end().trim_end_matches('\\'));
        let open_span = !fenced && pending.matches('`').count() % 2 == 1;
        if !continued && !open_span {
            flush(&mut pending);
        }
    }
    flush(&mut pending);
    lines
}

/// The invocation that starts at `rhei transition`: it ends at the backtick
/// closing its code span, at the next invocation, or where its line does —
/// whichever comes first. Nothing beyond that end can excuse it.
fn command_span(line: &str, start: usize) -> &str {
    let rest = &line[start..];
    let mut end = rest.find('`').unwrap_or(rest.len());
    if let Some(next) = rest[1..end].find("rhei transition") {
        end = next + 1;
    }
    &rest[..end]
}

/// Every invocation in `text` that shows `--to` without the required `--from`.
/// A mention of the command by name (`cancel it with `rhei transition``) is not
/// an invocation, because its span carries no `--to`.
pub(crate) fn offending_invocations(text: &str) -> Vec<String> {
    let mut offenders = Vec::new();
    for line in logical_lines(text) {
        for (offset, _) in line.match_indices("rhei transition") {
            let command = command_span(&line, offset);
            if command.contains("--to") && !command.contains("--from") {
                offenders.push(command.to_string());
            }
        }
    }
    offenders
}

fn text_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read template directory") {
        let entry = entry.expect("template entry");
        let path = entry.path();
        if entry.file_type().expect("file type").is_dir() {
            text_files(&path, out);
        } else if fs::read_to_string(&path).is_ok() {
            out.push(path);
        }
    }
}

#[test]
fn every_template_transition_invocation_names_from() {
    let templates_root = repo_root().join("crates/rhei-cli/templates");
    let mut files = Vec::new();
    text_files(&templates_root, &mut files);
    assert!(!files.is_empty(), "no template files found under {}", templates_root.display());

    let mut offenders = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path).expect("read template");
        let rel = path.strip_prefix(&templates_root).unwrap_or(path);
        offenders.extend(
            offending_invocations(&text).into_iter().map(|c| format!("{}: {c}", rel.display())),
        );
    }

    assert!(
        offenders.is_empty(),
        "these templates print a `rhei transition` invocation without the required \
         `--from` (the CLI exits 2 on it):\n{}",
        offenders.join("\n")
    );
}

/// Guard: no template source carries a ticket's budget identity.
///
/// A hand-applied edge establishes the project's account and writes the moving
/// ticket's `budgetTicketId` into the plan it moved
/// ([§FS-rhei-budgets.5.4](../../docs/functional-spec/rhei-budgets.spec.md#54-absent-damaged-adopted)),
/// and a template source is a tracked plan: driving one by hand to try it out
/// leaves a committable identity behind. `rhei instantiate` refuses a template
/// that carries one — a budget identity belongs to a run and never to a template
/// ([§FS-rhei-library.5](../../docs/functional-spec/rhei-library.spec.md#5-placement-and-ticket-identity)) —
/// so the accident ships as a user's `rhei instantiate` failing. Nothing else in
/// this repository reads a template plan's metadata, which is why it is caught
/// here.
// §FS-rhei-library.5 §FS-rhei-budgets.5.4
#[test]
fn no_template_source_carries_a_budget_ticket_identity() {
    let roots = ["crates/rhei-cli/templates", ".agent-grounds/rhei/templates"];
    let mut offenders = Vec::new();
    for root in roots {
        let root = repo_root().join(root);
        let mut files = Vec::new();
        text_files(&root, &mut files);
        assert!(!files.is_empty(), "no template files found under {}", root.display());
        for path in files {
            if fs::read_to_string(&path).expect("read template").contains("budgetTicketId") {
                offenders.push(path.display().to_string());
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a budget identity belongs to a run and never to a template, and `rhei instantiate` \
         refuses one that carries it. Remove the key from:\n{}",
        offenders.join("\n")
    );
}

/// The guard reads one invocation at a time: a `--from` that belongs to a
/// neighbouring command, or to the prose around it, never excuses a defective
/// one, and a command the prose wrapped is still read whole.
#[test]
fn the_guard_reads_one_invocation_at_a_time() {
    let fenced_pair = "```bash\n\
        rhei transition <plan> --task <child> --to cancelled --result \"why\"\n\
        rhei transition <plan> --task <parent> --from supervising --to supervising\n\
        ```\n";
    assert_eq!(
        offending_invocations(fenced_pair),
        vec!["rhei transition <plan> --task <child> --to cancelled --result \"why\"".to_string()],
        "the second line's `--from` must not excuse the first line's cancel"
    );

    let wrapped = "- The supervisor cancels with `rhei transition <id> --from <current-state>\n  \
        --to cancelled --result \"<why>\"`. That state is the one in brackets.\n";
    assert!(
        offending_invocations(wrapped).is_empty(),
        "an invocation the prose wrapped inside one code span is read whole"
    );

    let mention = "Cancel it with `rhei transition`, and remember that `--to cancelled` \
        is what a skipped round gets.\n";
    assert!(offending_invocations(mention).is_empty(), "naming the command is not invoking it");

    // An em dash swept across the offsets a fixed window used to end at: a
    // byte-indexed slice cuts one of them mid-character and panics instead of
    // reporting the offender it is standing in.
    for pad in 100..300 {
        let long = format!(
            "rhei transition <plan> --task <child> --to cancelled --result \"{}\"—done\n",
            "x".repeat(pad)
        );
        assert_eq!(
            offending_invocations(&long).len(),
            1,
            "a multi-byte character is not a boundary"
        );
    }
}

/// A command the shell continued across source lines is one invocation: a
/// `--from` on the continuation belongs to it, and a missing one is still the
/// defect #117 reports. Inside a fence an unmatched backtick joins nothing.
#[test]
fn the_guard_reads_a_continued_command_whole() {
    let split_cancel = r#"
        ```bash
        rhei transition <plan> --task <child> \
          --to cancelled --result "why"
        ```
    "#;
    assert_eq!(
        offending_invocations(split_cancel),
        vec![r#"rhei transition <plan> --task <child> --to cancelled --result "why""#.to_string()],
        "a defective invocation the shell continued is still one invocation"
    );

    let split_correct = r#"
        ```bash
        rhei transition <id> --to cancelled \
          --from review
        ```
    "#;
    assert!(
        offending_invocations(split_correct).is_empty(),
        "the `--from` on a continuation line belongs to the command it continues"
    );

    let commented_fence = r#"
        ```bash
        # see ` for details
        rhei transition <a> --to cancelled
        # --from is required
        ```
    "#;
    assert_eq!(
        offending_invocations(commented_fence),
        vec!["rhei transition <a> --to cancelled".to_string()],
        "an unmatched backtick in a fence must not lend the comment's flag to the command"
    );

    let stray_tick = "A stray ` tick in prose.\n\
        rhei transition <id> --to cancelled\n\n\
        Later prose mentions --from <state>.\n";
    assert_eq!(
        offending_invocations(stray_tick),
        vec!["rhei transition <id> --to cancelled".to_string()],
        "a paragraph the command does not belong to cannot excuse it"
    );
}
