//! `rhei new` refuses every task metadata field in a description, not a sample
//! of them: each one, through each way a description arrives, as the first line
//! and after prose, gets the argument error and leaves the plan as it was.
// §FS-rhei-new.3.4.2

use std::fs;

use rhei_core::tokens::TASK_METADATA_FIELDS;

use super::new_tests::{flattened_output, project_with_rhei};
use super::*;

/// A value the parser would accept after each marker, keyed by the marker. A
/// field the grammar adds has no row here until someone writes one, and
/// [`task_metadata_lines`] fails rather than leave that field untested. It fails
/// too for a row whose marker the set no longer holds, so the table is a list of
/// its own: `**Excludes:**` is pinned here, not only through the set under test.
// §FS-rhei-new.3.4.2
const SAMPLE_VALUES: [(&str, &str); 12] = [
    ("**State:**", "completed"),
    ("**States:**", "default"),
    ("**Prior:**", "Task 1"),
    ("**Inherits:**", "reviewed from prior"),
    ("**Provides:**", "api-contract"),
    ("**Consumes:**", "1:api-contract"),
    ("**Excludes:**", "checkout=secret.md"),
    ("**Assignee:**", "agent-1"),
    ("**Model:**", "claude-opus-4-7"),
    ("**Target:**", "cld"),
    ("**MCP servers:**", "github"),
    ("**Skills:**", "review"),
];

/// Every field of the closed task metadata block, as rhei-core names the set,
/// and the retired `**States:**`, each followed by its sample value.
// §FS-rhei-new.3.4.2 §FS-rhei-plan-language.2
fn task_metadata_lines() -> Vec<String> {
    let markers: Vec<&str> = TASK_METADATA_FIELDS.into_iter().chain(["**States:**"]).collect();
    for (sampled, _) in SAMPLE_VALUES {
        assert!(
            markers.contains(&sampled),
            "{sampled} has a row in SAMPLE_VALUES, but TASK_METADATA_FIELDS does not hold it"
        );
    }
    markers
        .into_iter()
        .map(|marker| {
            let value = SAMPLE_VALUES
                .iter()
                .find_map(|(sampled, value)| (*sampled == marker).then_some(*value))
                .unwrap_or_else(|| {
                    panic!("{marker} has no row in SAMPLE_VALUES; add a value the parser accepts")
                });
            format!("{marker} {value}")
        })
        .collect()
}

/// The refusal `**Prior:**` gets today, which every field must get too.
const REFUSAL: &str = "would be read as plan structure rather than as description";

/// The two ways a description arrives: the flag's value, or standard input.
#[derive(Clone, Copy)]
enum Channel {
    Flag,
    Stdin,
}

impl Channel {
    fn flag(self) -> &'static str {
        match self {
            Channel::Flag => "--description",
            Channel::Stdin => "--description-file -",
        }
    }
}

/// A project with one plan holding one ticket, the shape the report used.
fn project_with_one_ticket(prefix: &str) -> TestDir {
    let dir = project_with_rhei(prefix);
    assert_success(&new_run_description(&dir, "First", Channel::Flag, "first body"));
    dir
}

/// `rhei new <title> --under auth` with `body` as its description.
fn new_run_description(dir: &Path, title: &str, channel: Channel, body: &str) -> CliRun {
    use std::io::Write;
    use std::process::Stdio;

    let mut command = rhei_command(dir.join(".home"));
    command.current_dir(dir).args(["new", title, "--under", "auth"]);
    match channel {
        Channel::Flag => {
            let output = command.args(["--description", body]).output().expect("rhei runs");
            CliRun::from(&output)
        }
        Channel::Stdin => {
            let mut child = command
                .args(["--description-file", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("rhei starts");
            child.stdin.take().expect("piped stdin").write_all(body.as_bytes()).expect("stdin");
            CliRun::from(&child.wait_with_output().expect("rhei runs"))
        }
    }
}

/// Run every field at line `line` of `body_for(field)` through both channels,
/// and name each create that was not refused with the argument error and the
/// plan left byte-identical. A create that got through is undone, so each one
/// starts from the same plan.
fn creates_not_refused(prefix: &str, line: usize, body_for: impl Fn(&str) -> String) -> String {
    let dir = project_with_one_ticket(prefix);
    let plan = dir.join("auth.rhei.md");
    let before = fs::read_to_string(&plan).expect("rhei file");
    let mut missed = Vec::new();
    for field in task_metadata_lines() {
        for channel in [Channel::Flag, Channel::Stdin] {
            let result = new_run_description(&dir, "Second", channel, &body_for(&field));
            let said = flattened_output(&result);
            let named = format!("line {line} of {}", channel.flag());
            let changed = fs::read_to_string(&plan).expect("rhei file") != before;
            let refused = !result.status.success()
                && !changed
                && said.contains(REFUSAL)
                && said.contains(&named)
                && !said.contains("PARSE ERROR");
            if !refused {
                // The error's headline, past any rollback note printed before it.
                let headline = said.find("× ").map_or(said.as_str(), |at| &said[at..]);
                let shown: String = headline.chars().take(160).collect();
                missed.push(format!(
                    "  {field} via {}: exit {:?}, plan changed: {changed}; said: {shown}",
                    channel.flag(),
                    result.status.code()
                ));
            }
            if changed {
                fs::write(&plan, &before).expect("restore the plan");
            }
        }
    }
    missed.join("\n")
}

/// §FS-rhei-new.3.4.2: a description whose first line opens with any metadata
/// field is refused. Unrefused, it is a live field of the new ticket and the
/// create exits 0 — an `**Excludes:**` withholds a file from its agent unseen.
#[test]
fn every_metadata_field_opening_a_description_is_refused() {
    let missed = creates_not_refused("new-desc-every-field", 1, |field| format!("{field}\nbody\n"));
    assert!(
        missed.is_empty(),
        "every metadata field opening a description must be refused with {REFUSAL:?}, \
         naming line 1, with the plan unchanged; these were not:\n{missed}"
    );
}

/// §FS-rhei-new.3.4.2: after a prose line the field is refused by the same
/// argument error, not by the rolled-back parse error with a line number in a
/// plan the author never opened.
#[test]
fn a_metadata_field_after_prose_is_refused_as_an_argument() {
    let missed = creates_not_refused("new-desc-field-after-prose", 2, |field| {
        format!("prose first\n{field}\n")
    });
    assert!(
        missed.is_empty(),
        "every metadata field after a prose line must be refused with {REFUSAL:?}, \
         naming line 2, not with a parse error; these were not:\n{missed}"
    );
}
