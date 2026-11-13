//! A node-shaped heading before `## Tasks`: prose when nothing follows it,
//! refused at the heading's line, naming `## Tasks`, when task metadata does.
//! §FS-rhei-errors.1.7

use super::*;

/// The plan of #685: a task heading and its state, and no `## Tasks` above them.
const ITALY: &str = "# Rhei: Italy\n\n### Task 1: Book the trains\n**State:** in-progress\n";

#[test]
#[ignore = "red until #685: the refusal names the missing '## Tasks' at the heading's line; implement removes this"]
fn a_task_heading_with_no_tasks_section_is_refused_at_the_heading_naming_tasks() {
    let err = parse(ITALY).unwrap_err();

    assert_eq!(err.line, Some(3), "the heading's line, not the field's: {err:?}");
    assert!(err.message.contains("## Tasks"), "must name the missing section: {err:?}");
    assert!(
        !err.message.contains("Metadata field appears outside a task"),
        "the field sits under a task heading: {err:?}"
    );
}

#[test]
#[ignore = "red until #685: a task heading in a content section is named at its own line; implement removes this"]
fn a_task_heading_in_a_content_section_before_tasks_is_refused_at_the_heading() {
    let input = "# Rhei: Italy\n\n## Notes\n\n### Task 1: Book the trains\n**State:** pending\n\n\
                 ## Tasks\n\n### Task 2: Pack\n**State:** pending\n";
    let err = parse(input).unwrap_err();

    assert_eq!(err.line, Some(5), "the heading's line, not the field's: {err:?}");
    assert!(err.message.contains("## Tasks"), "must name the section it belongs under: {err:?}");
}

#[test]
#[ignore = "red until #685: validate lists the missing section once, as the cause; implement removes this"]
fn collecting_reports_the_missing_tasks_section_once() {
    let (rhei, errors) = parse_collect(ITALY);

    assert!(rhei.is_none());
    assert_eq!(errors.len(), 1, "one cause, not two distinct faults: {errors:?}");
    assert_eq!(errors[0].line, Some(3), "{errors:?}");
    assert!(errors[0].message.contains("## Tasks"), "{errors:?}");
}

#[test]
fn a_field_with_no_node_heading_above_it_keeps_the_plain_refusal() {
    let input = "# Rhei: Italy\n\n## Notes\n\n**State:** pending\n\n## Tasks\n";
    let err = parse(input).unwrap_err();

    assert_eq!(err.message, "Metadata field appears outside a task");
    assert_eq!(err.line, Some(5));
}

#[test]
fn prose_node_shaped_headings_in_a_content_section_still_parse() {
    let input = "# Rhei: Italy\n\n## Notes\n\n### Step 1: Gather the tickets\n\nPrint them.\n\n\
                 ### Task 1: an example heading\n\nNo metadata here.\n\n\
                 ## Tasks\n\n### Task 1: Book the trains\n**State:** pending\n";
    let rhei = parse(input).expect("prose headings in a content section are content");

    assert_eq!(rhei.tasks.len(), 1);
    assert!(rhei.content_sections[0].content.contains("### Step 1: Gather the tickets"));
    assert!(rhei.content_sections[0].content.contains("### Task 1: an example heading"));
}
