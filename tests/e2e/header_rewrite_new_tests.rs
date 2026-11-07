//! `rhei new`'s depth refusal says what to do about the frontmatter the parser read, so
//! a `structure:` line quoted or written anywhere else in the document does not move its
//! help, and a block written in either YAML form is found (agent-grounds/rhei#581).

// §FS-rhei-plan-language.2.3 §FS-rhei-new.3.3

use super::new_tests::{flattened_output, new_run};
use super::reset_invalid_plan_support::{assert_validates, plan_world, MACHINE};
use super::reset_remedy_support::RemedyWorld;

/// Two levels of tickets: the default limit, so one more is refused either way.
const NESTED: &str = "### Task 1: One\n**State:** open\n\n#### Task 1.1: Two\n**State:** open\n";

const NO_FRONTMATTER: &str = "this rhei declares no frontmatter";
const RAISE_THE_FIELD: &str = "raise `structure.maxLevels` to 3 in the rhei's frontmatter";

/// A flow-style block, one line of YAML with no `structure:` alone on a line.
const FLOW_BLOCK: &str = "---\nstructure: {maxLevels: 2}\n---\n";

/// The same limit as a block-style mapping.
const BLOCK_STYLE: &str = "---\nstructure:\n  maxLevels: 2\n---\n";

/// A subtask under `under`, on a plan that validates, is refused at depth 3 and its help
/// is `help` rather than the other one.
#[track_caller]
fn assert_refusal_helps(world: &RemedyWorld, under: &str, help: &str) {
    assert_validates(world);
    let target = world.target.to_str().expect("a utf-8 path");
    let result = new_run(&["new", "Deep", "--project", target, "--under", under], &world.root);
    let said = flattened_output(&result);
    assert!(!result.status.success(), "the ticket is too deep:\n{said}");
    assert!(said.contains("allows 2"), "the refusal itself stays:\n{said}");
    assert!(said.contains(help), "expected {help:?}, got:\n{said}");
    let other = if help == NO_FRONTMATTER { RAISE_THE_FIELD } else { NO_FRONTMATTER };
    assert!(!said.contains(other), "not {other:?}, got:\n{said}");
}

#[track_caller]
fn assert_plan_help(name: &str, above_tasks: &str, help: &str) {
    let plan = format!("{above_tasks}\n## Tasks\n\n{NESTED}");
    assert_refusal_helps(&plan_world(name, MACHINE, &plan), "plan.1.1", help);
}

#[test]
fn a_plain_header_has_no_frontmatter_to_raise() {
    assert_plan_help("header-new-plain", "# Rhei: A\n", NO_FRONTMATTER);
}

#[test]
fn a_structure_line_in_a_section_is_not_the_frontmatter() {
    let above = "# Rhei: A\n\n## Notes\n\nThe key is written like this:\n\nstructure:\n";
    assert_plan_help("header-new-section", above, NO_FRONTMATTER);
}

#[test]
fn a_structure_line_quoted_above_the_header_is_not_the_frontmatter() {
    let above = "```\nstructure:\n```\n\n# Rhei: A\n";
    assert_plan_help("header-new-quoted", above, NO_FRONTMATTER);
}

#[test]
fn a_flow_style_block_declares_the_structure() {
    let above = format!("# Rhei: A\n{FLOW_BLOCK}");
    assert_plan_help("header-new-flow", &above, RAISE_THE_FIELD);
}

#[test]
fn a_block_style_mapping_declares_the_structure() {
    let above = format!("# Rhei: A\n{BLOCK_STYLE}");
    assert_plan_help("header-new-block", &above, RAISE_THE_FIELD);
}

/// A workspace reads its index, whose parse answers the same way.
#[test]
fn an_index_answers_from_its_own_block() {
    for (name, index, help) in [
        ("header-new-index-flow", format!("# Rhei: WS\n{FLOW_BLOCK}"), RAISE_THE_FIELD),
        ("header-new-index-quoted", "```\nstructure:\n```\n# Rhei: WS\n".into(), NO_FRONTMATTER),
    ] {
        let world = RemedyWorld::at(name, "ws");
        world.write("states.yaml", MACHINE);
        world.write("index.rhei.md", &index);
        world.write("tasks/01-a.md", NESTED);
        assert_refusal_helps(&world, "ws.1.1", help);
    }
}
