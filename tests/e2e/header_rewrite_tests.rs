//! Every header the parser reads is the one a frontmatter write finds. Each test
//! validates first, so it shows `rhei validate` accepting what the verb then fails on
//! (agent-grounds/rhei#581). The controls run the same steps under `# Rhei: WS`, so
//! only the header differs between a control and a red test.

// §FS-rhei-plan-language.2.3 §FS-rhei-reset.2.3 §FS-rhei-panta.6

use std::fs;

use super::reset_invalid_plan_support::{
    assert_validates, edit, finish, listed_state, plan_world, MACHINE,
};
use super::reset_remedy_support::RemedyWorld;

const ONE_SPACE: &str = "# Rhei: WS";
const TWO_SPACES: &str = "#  Rhei: WS";
const TAB: &str = "#\tRhei: WS";
const NO_BREAK_SPACE: &str = "#\u{a0}Rhei: WS";

const TASK: &str = "### Task 1: First\n**State:** open\n";

/// A key the register has reset clear, seeded by hand because this machine counts no
/// visits. `budgetTicketId` is the key it keeps. §FS-rhei-transitions.2.5
const FRONTMATTER: &str =
    "---\nmetadata:\n  tasks:\n    1:\n      stateVisits:\n        open: 2\n---\n";

/// A plan that quotes the plan format above its own header.
const QUOTED: &str = "```markdown\n# Rhei: quoted\n```\n\n";

/// The same quotation, carrying the `---` a frontmatter write opens on.
const QUOTED_FENCE: &str = "```markdown\n# Rhei: quoted\n---\n```\n\n";

fn workspace(name: &str, index: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "ws");
    world.write("states.yaml", MACHINE);
    world.write("index.rhei.md", index);
    world.write("tasks/01-a.md", TASK);
    world
}

fn single_file(name: &str, above_tasks: &str) -> RemedyWorld {
    plan_world(name, MACHINE, &format!("{above_tasks}\n## Tasks\n\n{TASK}"))
}

fn project(name: &str, manifest: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "project");
    world.write("states.yaml", MACHINE);
    world.write("index.panta.md", manifest);
    world.write("basin/01-a.md", TASK);
    world
}

fn read(world: &RemedyWorld, relative: &str) -> String {
    fs::read_to_string(world.root.join(relative)).expect("read the plan file")
}

#[track_caller]
fn assert_ran(world: &RemedyWorld, subcommand: &str, args: &[&str]) {
    let run = world.rhei(subcommand, args);
    assert!(
        run.status.success(),
        "`rhei {subcommand} {args:?}` must succeed on a plan `rhei validate` accepts, \
         exited {:?}:\n{}{}",
        run.status.code(),
        run.stdout,
        run.stderr
    );
}

/// `rhei complete` finishes the ticket under the header it was given.
#[track_caller]
fn assert_completes(world: &RemedyWorld, task: &str, listed_as: &str) {
    assert_validates(world);
    assert_ran(world, "complete", &["--task", task, "--result", "finished", "--no-callbacks"]);
    assert_validates(world);
    assert_eq!(listed_state(world, listed_as), "done");
}

/// The ticket reopened with its result link gone, and the runtime gone with what it
/// held: the whole reset, and none of it left half done. §FS-rhei-reset.2.3
#[track_caller]
fn assert_reset_whole(world: &RemedyWorld, listed_as: &str, task_file: &str) {
    assert_validates(world);
    assert_ran(world, "reset", &["--dry-run"]);
    assert_ran(world, "reset", &["--yes"]);
    assert_eq!(listed_state(world, listed_as), "open");
    assert!(!read(world, task_file).contains("**Result:**"), "the result link is gone");
    assert!(!world.root.join("runtime").exists(), "`rhei reset --yes` removes runtime/");
}

/// Finished under one space, then the header edited in place to `header`, which keeps
/// the frontmatter `rhei complete` wrote: replacing the index would leave a reset with
/// nothing to rewrite there. §FS-rhei-reset.2
#[track_caller]
fn reset_workspace_under(name: &str, header: &str) {
    let world = workspace(name, &format!("{ONE_SPACE}\n{FRONTMATTER}"));
    finish(&world, "1", &[]);
    if header != ONE_SPACE {
        edit(&world, "index.rhei.md", ONE_SPACE, header);
    }

    assert_reset_whole(&world, "ws.1", "tasks/01-a.md");

    let index = read(&world, "index.rhei.md");
    assert!(index.starts_with(&format!("{header}\n")), "the author's header stays:\n{index}");
    assert!(!index.contains("stateVisits"), "reset clears the runtime metadata:\n{index}");
    assert!(index.contains("budgetTicketId"), "the register keeps the identity:\n{index}");
}

/// The same for a single-file plan, whose rewrite runs inside the simulation.
#[track_caller]
fn reset_single_file_under(name: &str, header: &str) {
    let world = single_file(name, &format!("{ONE_SPACE}\n"));
    finish(&world, "1", &[]);
    if header != ONE_SPACE {
        edit(&world, "plan.rhei.md", ONE_SPACE, header);
    }

    assert_reset_whole(&world, "plan.1", "plan.rhei.md");

    let plan = read(&world, "plan.rhei.md");
    assert!(plan.starts_with(&format!("{header}\n")), "the author's header stays:\n{plan}");
    assert!(plan.contains("budgetTicketId"), "the register keeps the identity:\n{plan}");
}

/// The frontmatter sits under the real header and the quotation above it is untouched.
/// §FS-rhei-plan-language.2.1
#[track_caller]
fn assert_written_under_the_real_header(text: &str, quotation: &str) {
    let (above, below) = text
        .split_once(&format!("{ONE_SPACE}\n"))
        .unwrap_or_else(|| panic!("the real header is still there:\n{text}"));
    assert_eq!(above, quotation, "the quotation stays byte for byte:\n{text}");
    let frontmatter = below
        .trim_start_matches('\n')
        .strip_prefix("---\n")
        .unwrap_or_else(|| panic!("the frontmatter comes next after the real header:\n{text}"));
    let (frontmatter, _) = frontmatter.split_once("---\n").expect("the frontmatter closes");
    assert!(frontmatter.contains("budgetTicketId"), "the write landed there:\n{text}");
}

#[test]
fn complete_under_one_space_is_the_control() {
    let world = workspace("header-complete-control", &format!("{ONE_SPACE}\n"));
    assert_completes(&world, "1", "ws.1");
}

#[test]
fn complete_finishes_a_ticket_under_two_spaces() {
    let world = workspace("header-complete-two", &format!("{TWO_SPACES}\n"));
    assert_completes(&world, "1", "ws.1");
    assert!(read(&world, "index.rhei.md").starts_with(&format!("{TWO_SPACES}\n---\n")));
}

#[test]
fn complete_finishes_a_ticket_under_a_tab() {
    let world = workspace("header-complete-tab", &format!("{TAB}\n"));
    assert_completes(&world, "1", "ws.1");
    assert!(read(&world, "index.rhei.md").starts_with(&format!("{TAB}\n---\n")));
}

/// The parser's `\s` is Unicode, so the rule is not only about ASCII blanks.
#[test]
fn complete_finishes_a_ticket_under_a_no_break_space() {
    let world = workspace("header-complete-nbsp", &format!("{NO_BREAK_SPACE}\n"));
    assert_completes(&world, "1", "ws.1");
}

#[test]
fn reset_of_a_workspace_under_one_space_is_the_control() {
    reset_workspace_under("header-reset-control", ONE_SPACE);
}

/// A dry run that promises a clean reset, and a real one that leaves the task reopened
/// with its result still under `runtime/`. §FS-rhei-reset.2.3
#[test]
fn reset_of_a_workspace_under_two_spaces_is_whole() {
    reset_workspace_under("header-reset-two", TWO_SPACES);
}

#[test]
fn single_file_plan_under_one_space_is_the_control() {
    let world = single_file("header-single-control", &format!("{ONE_SPACE}\n"));
    assert_completes(&world, "1", "plan.1");
}

#[test]
fn complete_finishes_a_single_file_plan_under_two_spaces() {
    let world = single_file("header-single-two", &format!("{TWO_SPACES}\n"));
    assert_completes(&world, "1", "plan.1");
}

#[test]
fn reset_of_a_single_file_plan_under_one_space_is_the_control() {
    reset_single_file_under("header-single-reset-control", ONE_SPACE);
}

/// Here the rewrite runs inside the simulation, so the reset refuses before its preview
/// and changes nothing, `--dry-run` included. It is a plan that can never be reset.
#[test]
fn reset_of_a_single_file_plan_under_two_spaces_is_whole() {
    reset_single_file_under("header-single-reset-two", TWO_SPACES);
}

#[test]
fn complete_under_a_one_space_manifest_is_the_control() {
    let world = project("header-panta-control", "# Panta: P\n");
    assert_completes(&world, "basin.1", "basin.1");
    assert!(read(&world, "index.panta.md").contains("budgetTicketId"));
}

/// The basin has no index of its own, so its metadata is written to the manifest.
/// §FS-rhei-panta.6
#[test]
fn complete_writes_the_metadata_of_a_basin_ticket_under_a_two_space_manifest() {
    let world = project("header-panta-two", "#  Panta: P\n");
    assert_completes(&world, "basin.1", "basin.1");
    let manifest = read(&world, "index.panta.md");
    assert!(manifest.starts_with("#  Panta: P\n---\n"), "written under it:\n{manifest}");
    assert!(manifest.contains("budgetTicketId"), "the basin ticket's own key:\n{manifest}");
}

/// Today the write lands inside the fence, where nothing reads it, and validation still
/// passes. §FS-rhei-plan-language.2.1
#[test]
fn complete_writes_a_workspace_index_under_its_real_header_not_a_quoted_one() {
    let world = workspace("header-fence-index", &format!("{QUOTED}{ONE_SPACE}\n"));
    assert_completes(&world, "1", "ws.1");
    assert_written_under_the_real_header(&read(&world, "index.rhei.md"), QUOTED);
}

#[test]
fn complete_writes_a_single_file_plan_under_its_real_header_not_a_quoted_one() {
    let world = single_file("header-fence-single", &format!("{QUOTED}{ONE_SPACE}\n"));
    assert_completes(&world, "1", "plan.1");
    assert_written_under_the_real_header(&read(&world, "plan.rhei.md"), QUOTED);
}

/// With no `---` below it to close on, the quoted `---` reads as frontmatter left open.
#[test]
fn complete_is_not_refused_for_a_frontmatter_fence_in_a_quotation() {
    let world = workspace("header-fence-open", &format!("{QUOTED_FENCE}{ONE_SPACE}\n"));
    assert_completes(&world, "1", "ws.1");
    assert_written_under_the_real_header(&read(&world, "index.rhei.md"), QUOTED_FENCE);
}

/// The write spans from the quoted `---` to the real frontmatter's, which takes the
/// fence's close and the real header with it: exit 0, and a plan that no longer parses.
#[test]
fn complete_keeps_the_real_header_below_a_frontmatter_fence_in_a_quotation() {
    let index = format!("{QUOTED_FENCE}{ONE_SPACE}\n---\nowner: me\n---\n");
    let world = workspace("header-fence-kept", &index);
    assert_completes(&world, "1", "ws.1");
    let text = read(&world, "index.rhei.md");
    assert_written_under_the_real_header(&text, QUOTED_FENCE);
    assert!(text.contains("owner: me"), "the author's frontmatter stays:\n{text}");
}

/// The same span taken by a reset, through its clear of the index's runtime metadata.
#[test]
fn reset_keeps_the_real_header_below_a_frontmatter_fence_in_a_quotation() {
    let index = format!("{QUOTED_FENCE}{ONE_SPACE}\n{FRONTMATTER}");
    let world = workspace("header-fence-reset", &index);
    assert_validates(&world);
    assert_ran(&world, "reset", &["--yes"]);
    assert_validates(&world);
    let text = read(&world, "index.rhei.md");
    assert!(text.starts_with(QUOTED_FENCE), "the quotation stays byte for byte:\n{text}");
    assert!(text.contains(&format!("\n{ONE_SPACE}\n")), "the real header stays:\n{text}");
    assert!(!text.contains("stateVisits"), "reset clears the runtime metadata:\n{text}");
}
