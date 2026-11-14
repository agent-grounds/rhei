//! A task metadata line above `## Tasks` is refused whichever field it is and
//! whatever follows its marker, so a task written under a content section is
//! never absorbed into that section's text and lost (agent-grounds/rhei#719).
//! §FS-rhei-plan-language.1.1.1

use super::*;

/// The refusal on `main` today. agent-grounds/rhei#685 rewords it; if that lands
/// first, this is the one place to retarget.
const OUTSIDE_A_TASK: &str = "Metadata field appears outside a task";

/// One valued line per kind of task field. `**State:**` comes first: it is the
/// control, refused in every shape below before the fix as after it.
const VALUED_FIELDS: [&str; 5] = [
    "**State:** pending",
    "**Model:** sample",
    "**Target:** sample",
    "**MCP servers:** github",
    "**Skills:** release-notes",
];

/// The reporter's plan: a task heading under `## Notes`, its field on line 5.
fn under_a_misplaced_heading(field: &str) -> String {
    format!(
        "# Rhei: Italy\n\n## Notes\n### Task 1: Misplaced\n{field}\n\n\
         ## Tasks\n\n### Task 2: Real\n**State:** pending\n"
    )
}

/// The field in a content section with no heading above it, on line 4.
fn in_a_content_section(field: &str) -> String {
    format!(
        "# Rhei: Italy\n\n## Notes\n{field}\n\n## Tasks\n\n### Task 2: Real\n**State:** pending\n"
    )
}

/// The field before the first content section, on line 3.
fn before_any_section(field: &str) -> String {
    format!("# Rhei: Italy\n\n{field}\n\n## Tasks\n\n### Task 2: Real\n**State:** pending\n")
}

/// What went wrong with `input`, or `None` when it is refused as the spec says:
/// with the refusal above, at `line`.
fn miss(input: &str, line: usize) -> Option<String> {
    match parse(input) {
        Ok(plan) => {
            let ids: Vec<String> = plan.tasks.iter().map(|task| task.id.to_string()).collect();
            let text: Vec<&str> =
                plan.content_sections.iter().map(|section| section.content.as_str()).collect();
            Some(format!("accepted, tasks {ids:?}, section text {text:?}"))
        }
        Err(err) if err.message == OUTSIDE_A_TASK && err.line == Some(line) => None,
        Err(err) => Some(format!("refused at line {:?} with {:?}", err.line, err.message)),
    }
}

fn misses(shape: fn(&str) -> String, line: usize) -> Vec<String> {
    VALUED_FIELDS
        .iter()
        .filter_map(|field| miss(&shape(field), line).map(|why| format!("{field}: {why}")))
        .collect()
}

/// The ticket's case: the field under the misplaced heading decides nothing.
#[test]
#[ignore = "red until #719: a valued Model, Target, MCP servers or Skills line under a misplaced \
            task heading is absorbed into the section; implement removes this"]
fn every_valued_task_field_under_a_misplaced_heading_is_refused_at_its_line() {
    let missed = misses(under_a_misplaced_heading, 5);
    assert!(missed.is_empty(), "each must be refused at line 5:\n{}", missed.join("\n"));
}

/// The remedy covers a field line with no task heading above it too.
#[test]
#[ignore = "red until #719: a valued Model, Target, MCP servers or Skills line above ## Tasks \
            with no heading is absorbed into the section; implement removes this"]
fn every_valued_task_field_above_tasks_is_refused_without_a_heading() {
    let mut missed = misses(in_a_content_section, 4);
    missed.extend(misses(before_any_section, 3));
    assert!(missed.is_empty(), "each must be refused at its line:\n{}", missed.join("\n"));
}

/// **Guard.** Inside a task the execution overrides keep today's meaning: a
/// valued line sets the override, a bare or malformed one is refused as
/// malformed, never as outside a task. §FS-rhei-plan-language.3.11
#[test]
fn execution_overrides_inside_a_task_keep_their_meaning() {
    let task = |field: &str| {
        format!("# Rhei: Italy\n\n## Notes\n\n## Tasks\n\n### Task 1: Real\n**State:** pending\n{field}\n")
    };
    let plan = parse(&task("**Model:** sample")).expect("a valued Model parses");
    assert_eq!(plan.tasks[0].model.as_deref(), Some("sample"));
    let plan = parse(&task("**Target:** cdx:openai:gpt-5")).expect("a valued Target parses");
    assert_eq!(plan.tasks[0].target.as_deref(), Some("cdx:openai:gpt-5"));
    for (field, message) in [
        ("**Model:**", "Malformed metadata field: expected '**Model:** <value>'"),
        ("**Model** sample", "Malformed metadata field: expected '**Model:** <value>'"),
        ("**Target:**", "Malformed metadata field: expected '**Target:** <value>'"),
        ("**Target** sample", "Malformed metadata field: expected '**Target:** <value>'"),
    ] {
        let err = parse(&task(field)).expect_err(field);
        assert_eq!((err.message.as_str(), err.line), (message, Some(9)), "{field}");
    }
}

/// **Guard.** A fenced example in a content section quotes the plan format and
/// stays content, whichever fields it shows. §FS-rhei-plan-language.2.1
#[test]
fn a_fenced_task_example_above_tasks_stays_content() {
    let example = "```markdown\n### Task 1: Example\n**State:** pending\n**Model:** sample\n\
                   **MCP servers:** github\n**Skills:** release-notes\n```";
    let input = format!(
        "# Rhei: Italy\n\n## Notes\n{example}\n\n## Tasks\n\n### Task 2: Real\n**State:** pending\n"
    );
    let plan = parse(&input).expect("a fenced example is content");
    assert_eq!(plan.tasks.len(), 1, "the example must not become a task");
    assert!(plan.content_sections[0].content.contains("**Model:** sample"));
}

/// **Guard.** A bold label outside the task metadata vocabulary is prose, even
/// one that begins like a field name.
#[test]
fn bold_labels_outside_the_vocabulary_stay_content() {
    let labels = "**Note:** keep it short\n**Target audience:** operators\n\
                  **Models:** small, large\n**Skillset:** reading";
    let input = format!(
        "# Rhei: Italy\n\n## Notes\n{labels}\n\n## Tasks\n\n### Task 2: Real\n**State:** pending\n"
    );
    let plan = parse(&input).expect("labels outside the vocabulary are content");
    assert_eq!(plan.content_sections[0].content.trim(), labels);
}
