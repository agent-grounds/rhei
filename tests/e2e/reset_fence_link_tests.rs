//! The two halves of a run a reset undoes, against the plans a fence makes hard: where
//! `rhei complete` puts a task's result link when the task quotes the plan format, and
//! whether the reset finds that link again (agent-grounds/rhei#580). A reset takes every
//! fenced line for content, so a link it is to remove has to be written outside the fence.
//!
//! Also the task files a reset reads as task files: a `# Rhei:` line and a `---` in one
//! are body text, because a task file has no header for them to belong to.

// §FS-rhei-reset.2.4 §FS-rhei-complete.3.3 §FS-rhei-plan-language.2.1

use std::fs;

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;

/// A world, what each layout calls its plan, and how it resets.
struct Plan {
    world: RemedyWorld,
    relative: &'static str,
    authored: String,
    reset: &'static [&'static str],
}

const RESET_PLAN: [&str; 3] = ["--rhei", "plan", "--yes"];
const RESET_WORKSPACE: [&str; 1] = ["--yes"];

/// The tests' file, with its plan's own header outside what they compare.
const FILE_HEAD: &str = "# Rhei: Plan\n\n## Tasks\n\n";

impl Plan {
    /// `tasks` as a lone single-file rhei.
    fn single_file(name: &str, tasks: &str) -> Self {
        let authored = format!("{FILE_HEAD}{tasks}");
        let world = plan_world(name, MACHINE, &authored);
        Plan { world, relative: "plan.rhei.md", authored, reset: &RESET_PLAN }
    }

    /// `tasks` as the one task file of a Directory Workspace.
    fn workspace(name: &str, tasks: &str) -> Self {
        let world = RemedyWorld::at(name, "wsplan");
        world.write("states.yaml", MACHINE);
        world.write("index.rhei.md", "# Rhei: Wsplan\n");
        world.write("tasks/01-a.md", tasks);
        Plan {
            world,
            relative: "tasks/01-a.md",
            authored: tasks.to_string(),
            reset: &RESET_WORKSPACE,
        }
    }

    fn read(&self) -> String {
        fs::read_to_string(self.world.root.join(self.relative)).expect("read the plan file")
    }

    fn links(&self) -> usize {
        self.read().lines().filter(|line| line.starts_with("> **Result:** [")).count()
    }

    #[track_caller]
    fn reset(&self) {
        let run = self.world.rhei("reset", self.reset);
        assert!(run.status.success(), "the reset refused:\n{}{}", run.stdout, run.stderr);
    }

    /// One complete and one reset, as many times as `cycles`: one link after each
    /// complete, valid; none after each reset, and the text as it was authored.
    #[track_caller]
    fn assert_cycles(&self, cycles: usize) {
        for cycle in 1..=cycles {
            finish(&self.world, "1", &[]);
            assert_validates(&self.world);
            assert_eq!(self.links(), 1, "after complete {cycle}:\n{}", self.read());
            self.reset();
            assert_eq!(self.links(), 0, "after reset {cycle}:\n{}", self.read());
            assert_validates(&self.world);
            self.assert_tasks_as_authored(&format!("after reset {cycle}"));
        }
    }

    /// The file from `## Tasks` down as authored. A single file's frontmatter is left
    /// out, because a reset keeps `budgetTicketId` there (§FS-rhei-reset.2).
    #[track_caller]
    fn assert_tasks_as_authored(&self, when: &str) {
        let tasks = |text: &str| text[text.find("## Tasks").unwrap_or(0)..].to_string();
        assert_eq!(tasks(&self.read()), tasks(&self.authored), "{when}");
    }

    /// Both a run's marks gone, by state and by link, as `rhei list` reads it.
    #[track_caller]
    fn assert_task_back_at_open(&self, ticket: &str) {
        assert_eq!(listed_state(&self.world, ticket), "open", "{}", self.read());
    }
}

/// The finding's plan: nothing below the opening line is outside the fence.
const UNCLOSED: &str = "### Task 1: A\n**State:** open\n\nBody\n\n```text\nunclosed\n";

/// Every line a reset or a completion could take for its own, all quoted in a fence
/// that is never closed.
const UNCLOSED_EVERY_SHAPE: &str = "### Task 1: A\n**State:** open\n\n```markdown\n\
    ### Task 9: Quoted\n**State:** done\n\n> **Result:** runtime/results/plan.7.md\n\
    **Assignee:** someone\n";

#[test]
fn a_link_goes_above_an_unclosed_fence_in_a_single_file_each_cycle() {
    Plan::single_file("reset-link-unclosed-file", UNCLOSED).assert_cycles(3);
}

#[test]
fn a_link_goes_above_an_unclosed_fence_in_a_workspace_each_cycle() {
    Plan::workspace("reset-link-unclosed-ws", UNCLOSED).assert_cycles(3);
}

#[test]
fn every_line_of_an_unclosed_fence_survives_in_a_single_file() {
    Plan::single_file("reset-link-unclosed-shapes-file", UNCLOSED_EVERY_SHAPE).assert_cycles(2);
}

#[test]
fn every_line_of_an_unclosed_fence_survives_in_a_workspace() {
    Plan::workspace("reset-link-unclosed-shapes-ws", UNCLOSED_EVERY_SHAPE).assert_cycles(2);
}

/// A tilde fence is one `rhei complete` has to find by the language's rule too, and a
/// shorter run of it inside closes nothing.
#[test]
fn a_link_goes_above_an_unclosed_tilde_fence() {
    let tasks = "### Task 1: A\n**State:** open\n\n~~~~markdown\n~~~\n### Task 9: Quoted\n";
    Plan::single_file("reset-link-unclosed-tilde-file", tasks).assert_cycles(2);
    Plan::workspace("reset-link-unclosed-tilde-ws", tasks).assert_cycles(2);
}

/// No blank line above the fence, so none is added for good: the reset gives back the
/// text as it was.
#[test]
fn a_fence_with_no_blank_line_above_it_comes_back_the_same() {
    let tasks = "### Task 1: A\n**State:** open\nBody\n```text\nunclosed\n";
    Plan::single_file("reset-link-unclosed-tight-file", tasks).assert_cycles(2);
    Plan::workspace("reset-link-unclosed-tight-ws", tasks).assert_cycles(2);
}

/// A task above the unclosed fence ends at a heading the parser reads, and that heading
/// is where its link goes: the fence is another task's.
#[test]
fn only_the_task_the_fence_is_in_has_its_link_above_it() {
    let tasks =
        "### Task 1: A\n**State:** open\n\n### Task 2: B\n**State:** open\n\n```text\nopen\n";
    let plan = Plan::single_file("reset-link-unclosed-other-file", tasks);
    finish(&plan.world, "1", &[]);
    finish(&plan.world, "2", &[]);
    assert_validates(&plan.world);
    let now = plan.read();
    let (one, two) = (now.find("[plan.1]").expect("link 1"), now.find("[plan.2]").expect("link 2"));
    assert!(one < now.find("### Task 2").expect("task 2"), "{now}");
    assert!(two < now.find("```text").expect("fence"), "{now}");
    plan.reset();
    plan.assert_tasks_as_authored("after the reset");
}

/// A closed fence quoting a heading, with the quoting task itself finished: `rhei
/// complete` has to read this fence by the language's rule too, because a link it wrote
/// inside would look to a reset exactly like a quotation (agent-grounds/rhei#614).
fn tilde_quote() -> String {
    "### Task 1: A\n**State:** open\n\n~~~markdown\n### Task 9: Example\n**State:** open\n~~~\n"
        .into()
}

fn nested_quote() -> String {
    "### Task 1: A\n**State:** open\n\n````markdown\n```\n### Task 9: Example\n**State:** open\n```\n\
     ````\n"
        .into()
}

#[test]
fn a_tilde_fence_quoting_a_heading_keeps_no_link_in_a_single_file() {
    Plan::single_file("reset-link-tilde-file", &tilde_quote()).assert_cycles(2);
}

#[test]
fn a_tilde_fence_quoting_a_heading_keeps_no_link_in_a_workspace() {
    Plan::workspace("reset-link-tilde-ws", &tilde_quote()).assert_cycles(2);
}

#[test]
fn a_nested_fence_quoting_a_heading_keeps_no_link_in_a_single_file() {
    Plan::single_file("reset-link-nested-file", &nested_quote()).assert_cycles(2);
}

#[test]
fn a_nested_fence_quoting_a_heading_keeps_no_link_in_a_workspace() {
    Plan::workspace("reset-link-nested-ws", &nested_quote()).assert_cycles(2);
}

/// R1-01's two files: a `# Rhei:` line above a task file's first task, with `---` under
/// it. `rhei validate` lists the tasks below them, so a reset has to move them.
#[test]
fn a_header_and_a_pair_of_rules_above_a_task_files_first_task_hide_nothing() {
    let tasks = "# Rhei: Notes\n\n---\n\n### Task 1: A\n**State:** open\n\n---\n\n\
                 ### Task 2: B\n**State:** open\n";
    let plan = Plan::workspace("reset-task-file-header-pair", tasks);
    assert_validates(&plan.world);
    plan.assert_cycles(1);
    plan.assert_task_back_at_open("wsplan.1");
}

#[test]
fn a_header_and_one_rule_above_a_task_files_first_task_hide_nothing() {
    let tasks = "# Rhei: Notes\n\n---\n\n### Task 1: A\n**State:** open\n";
    let plan = Plan::workspace("reset-task-file-header-one", tasks);
    assert_validates(&plan.world);
    plan.assert_cycles(1);
    plan.assert_task_back_at_open("wsplan.1");
}

/// YAML under a `# Rhei:` line in a task file is still body text, so a reset does not
/// rewrite it as the plan's frontmatter.
#[test]
fn yaml_under_a_header_in_a_task_file_stays_as_written() {
    let tasks = "# Rhei: Notes\n\n---\nbudgetTicketId: quoted\nmetadata:   {  }\n---\n\n\
                 ### Task 1: A\n**State:** open\n";
    let plan = Plan::workspace("reset-task-file-header-yaml", tasks);
    assert_validates(&plan.world);
    plan.assert_cycles(1);
}
