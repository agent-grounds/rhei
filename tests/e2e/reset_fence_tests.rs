//! A reset reads a plan's text by the language's one fence rule, as `rhei validate`
//! does, so a plan that quotes the plan format in a fence can be reset and the
//! quotation comes through byte-identical (agent-grounds/rhei#580).
//!
//! The quotations go through both layouts a reset rewrites, because the two fail
//! differently: the fence is last in a workspace task file, and followed by another
//! task in a single file. Most tests compare a plan's tasks to what was authored: the
//! real lines a reset owes a change to are gone, and nothing else moved. Frontmatter
//! is left out of that, because a reset keeps `budgetTicketId` there (§FS-rhei-reset.2).

// §FS-rhei-reset.2.4 §FS-rhei-plan-language.2.1

use std::fs;

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;

const RESET_PLAN: [&str; 3] = ["--rhei", "plan", "--yes"];
const RESET_WORKSPACE: [&str; 1] = ["--yes"];

/// The report's own: an example task with no state line.
const QUOTED_TASK: &str = "```markdown\n### Task 9: Example\n\nBody with no state line.\n```";

/// What a finished ticket looks like, with the blank line a result link is written
/// after.
const QUOTED_RUNTIME_LINES: &str =
    "```markdown\n**State:** done\n\n> **Result:** runtime/results/plan.7.md\n**Assignee:** someone\n```";

/// Every line a reset rewrites or removes, for the tests of the fence rule: whichever
/// pass reads the fence wrongly, one of these lines shows it.
const EVERY_SHAPE: &str =
    "### Task 9: Example\n\n> **Result:** runtime/results/plan.7.md\n**Assignee:** someone";

const NEXT_TASK: &str = "### Task 2: Next\n**State:** open\n";

/// Task 1 with `quote` in its body. The ticket a test finishes, so that its real
/// `**State:**` line and result link are what the reset has to get right.
fn quoting_task(quote: &str) -> String {
    format!("### Task 1: Write the docs\n**State:** open\n\nAn example of a task:\n\n{quote}\n")
}

/// Three tasks with `quote` in the middle one. `rhei complete` never reads that far
/// for Task 1, and it stops at Task 3's heading: it does not read this fence by the
/// language's rule (see agent-grounds/rhei#614), and a link it wrote inside a quotation
/// would be the reset's to delete, so these tests finish the tasks around the quote.
fn around_a_quote(quote: &str) -> [String; 3] {
    [
        "### Task 1: Finished\n**State:** open\n".to_string(),
        format!("### Task 2: Quotes\n**State:** open\n\n{quote}\n"),
        "### Task 3: After\n**State:** open\n".to_string(),
    ]
}

/// A file's tasks: from `## Tasks` in a single-file plan, whose frontmatter a run writes
/// to, and the whole of a workspace task file, whose block nothing does.
fn tasks_of(text: &str) -> &str {
    text.find("## Tasks").map_or(text, |at| &text[at..])
}

/// A plan as it was authored, and what a reset of it is given.
struct Authored {
    world: RemedyWorld,
    files: Vec<(String, String)>,
    reset: &'static [&'static str],
}

impl Authored {
    /// `plan` as a lone single-file rhei.
    fn single_file(name: &str, plan: String) -> Self {
        let world = plan_world(name, MACHINE, &plan);
        Authored { world, files: vec![("plan.rhei.md".into(), plan)], reset: &RESET_PLAN }
    }

    /// `quote` in a single-file plan, with a task after it.
    fn quoting_in_a_file(name: &str, quote: &str) -> Self {
        let plan = format!("# Rhei: Plan\n\n## Tasks\n\n{}\n{NEXT_TASK}", quoting_task(quote));
        Self::single_file(name, plan)
    }

    /// A Directory Workspace holding `index` and `tasks`, by file name.
    fn workspace(name: &str, index: &str, tasks: &[(&str, &str)]) -> Self {
        let world = RemedyWorld::at(name, "wsplan");
        world.write("states.yaml", MACHINE);
        world.write("index.rhei.md", index);
        let mut files = Vec::new();
        for (file, text) in tasks {
            let relative = format!("tasks/{file}");
            world.write(&relative, text);
            files.push((relative, (*text).to_string()));
        }
        Authored { world, files, reset: &RESET_WORKSPACE }
    }

    /// `quote` last in the file of the task it finishes.
    fn quoting_in_a_workspace(name: &str, quote: &str) -> Self {
        let task = quoting_task(quote);
        Self::workspace(name, "# Rhei: Wsplan\n", &[("01-a.md", &task), ("02-b.md", NEXT_TASK)])
    }

    /// Task 1 finished, `quote` in the unfinished task after it, and Task 3 after that:
    /// the fence between real lines on both sides, in a single file.
    fn around_a_quote_in_a_file(name: &str, quote: &str) -> Self {
        let plan = format!("# Rhei: Plan\n\n## Tasks\n\n{}", around_a_quote(quote).join("\n"));
        Self::single_file(name, plan)
    }

    /// The same three tasks, a file each.
    fn around_a_quote_in_a_workspace(name: &str, quote: &str) -> Self {
        let [first, second, third] = around_a_quote(quote);
        let tasks = [("01-a.md", first.as_str()), ("02-b.md", &second), ("03-c.md", &third)];
        Self::workspace(name, "# Rhei: Wsplan\n", &tasks)
    }

    /// Finish Task 1 through `rhei complete`, on a plan validate accepts on both sides
    /// of it: a plan it refused would be step 1's to refuse, not this point's.
    #[track_caller]
    fn finish_the_first_task(&self) {
        self.finish_tasks(&["1"]);
    }

    #[track_caller]
    fn finish_tasks(&self, tickets: &[&str]) {
        assert_validates(&self.world);
        for ticket in tickets {
            finish(&self.world, ticket, &[]);
        }
        assert_validates(&self.world);
    }

    #[track_caller]
    fn assert_reset_goes_through(&self) {
        let run = self.world.rhei("reset", self.reset);
        assert!(
            run.status.success(),
            "`rhei validate` accepts this plan, so the reset must go through; `rhei reset \
             {:?}` refused:\n{}{}",
            self.reset,
            run.stdout,
            run.stderr
        );
    }

    /// Every task as it was authored: back at its state, its result link gone, and
    /// every fenced line where it was.
    #[track_caller]
    fn assert_back_as_authored(&self) {
        for (relative, authored) in &self.files {
            let now = self.read(relative);
            assert_eq!(tasks_of(&now), tasks_of(authored), "{relative} after the reset");
        }
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.world.root.join(relative)).expect("read the plan file")
    }

    /// `relative` still holds the block scalar with the fence-shaped lines.
    #[track_caller]
    fn assert_holds_fence_shaped_lines(&self, relative: &str) {
        let now = self.read(relative);
        let lines: Vec<&str> = now.lines().map(str::trim).collect();
        assert!(
            lines.contains(&"```") && lines.contains(&"~~~"),
            "{relative} keeps its metadata:\n{now}"
        );
    }

    /// The whole of what a reset owes: finish, reset, and every file as authored.
    #[track_caller]
    fn assert_round_trip(&self) {
        self.assert_round_trip_finishing(&["1"]);
    }

    #[track_caller]
    fn assert_round_trip_finishing(&self, tickets: &[&str]) {
        self.finish_tasks(tickets);
        self.assert_reset_goes_through();
        self.assert_back_as_authored();
    }
}

/// The report's case 1.
#[test]
#[ignore = "red until #580: a fenced task heading stops a reset of a single-file plan; implement removes this"]
fn a_fenced_task_heading_with_no_state_line_does_not_stop_a_reset_of_a_single_file() {
    Authored::quoting_in_a_file("reset-fence-heading-file", QUOTED_TASK).assert_round_trip();
}

/// The fence is the last thing in the file, so the refusal has another wording here.
#[test]
#[ignore = "red until #580: a fenced task heading stops a reset of a workspace; implement removes this"]
fn a_fenced_task_heading_last_in_a_task_file_does_not_stop_a_reset_of_a_workspace() {
    Authored::quoting_in_a_workspace("reset-fence-heading-ws", QUOTED_TASK).assert_round_trip();
}

/// The report's case 3: the quotation is in a file with no task the reset moves, and
/// the file before it holds one.
#[test]
#[ignore = "red until #580: a fence in a file nothing moved in stops a workspace reset; implement removes this"]
fn a_fence_in_a_task_file_with_no_moved_task_does_not_stop_a_reset_of_a_workspace() {
    let first = "### Task 1: Finished\n**State:** open\n";
    let second = format!("### Task 2: Quotes\n**State:** open\n\n{QUOTED_TASK}\n");
    let third = "### Task 3: After\n**State:** open\n";
    let tasks = [("01-a.md", first), ("02-b.md", second.as_str()), ("03-c.md", third)];
    Authored::workspace("reset-fence-unmoved-ws", "# Rhei: Wsplan\n", &tasks).assert_round_trip();
}

/// The report's case 4, which nothing reports: the reset exits 0 and the lines are
/// gone. A real result link and a real claim are in the same plan, and still go.
#[test]
#[ignore = "red until #580: a reset that exits 0 deletes fenced result and assignee lines; implement removes this"]
fn fenced_result_and_assignee_lines_survive_a_reset_of_a_single_file_that_clears_real_ones() {
    let plan = Authored::quoting_in_a_file("reset-fence-runtime-file", QUOTED_RUNTIME_LINES);
    plan.finish_the_first_task();
    edit(&plan.world, "plan.rhei.md", NEXT_TASK, &format!("{NEXT_TASK}**Assignee:** me\n"));
    assert_validates(&plan.world);

    plan.assert_reset_goes_through();

    plan.assert_back_as_authored();
}

#[test]
#[ignore = "red until #580: a workspace reset that exits 0 deletes fenced result and assignee lines; implement removes this"]
fn fenced_result_and_assignee_lines_survive_a_reset_of_a_workspace() {
    Authored::quoting_in_a_workspace("reset-fence-runtime-ws", QUOTED_RUNTIME_LINES)
        .assert_round_trip();
}

/// A quoted heading naming the very task that moved, with a state under it. The real
/// line goes back and the quotation does not, on a reset that exits 0 either way.
#[test]
#[ignore = "red until #580: a reset rewrites a fenced state line under a fenced heading of a moved task; implement removes this"]
fn a_fenced_state_line_under_a_heading_naming_a_moved_task_is_not_rewritten_in_a_file() {
    let quote = "```markdown\n### Task 1: Example\n**State:** done\n```";
    Authored::quoting_in_a_file("reset-fence-moved-file", quote).assert_round_trip();
}

#[test]
#[ignore = "red until #580: a workspace reset rewrites a fenced state line of a moved task; implement removes this"]
fn a_fenced_state_line_under_a_heading_naming_a_moved_task_is_not_rewritten_in_a_workspace() {
    let quote = "```markdown\n### Task 1: Example\n**State:** done\n```";
    Authored::quoting_in_a_workspace("reset-fence-moved-ws", quote).assert_round_trip();
}

/// A tilde fence. A flag flipped on three backticks does not see it.
#[test]
#[ignore = "red until #580: a reset does not read a tilde fence; implement removes this"]
fn a_tilde_fence_is_a_fence_to_a_reset_of_a_single_file() {
    let quote = format!("~~~markdown\n{EVERY_SHAPE}\n~~~");
    Authored::around_a_quote_in_a_file("reset-fence-tilde-file", &quote)
        .assert_round_trip_finishing(&["1", "3"]);
}

#[test]
#[ignore = "red until #580: a workspace reset does not read a tilde fence; implement removes this"]
fn a_tilde_fence_is_a_fence_to_a_reset_of_a_workspace() {
    let quote = format!("~~~markdown\n{EVERY_SHAPE}\n~~~");
    Authored::around_a_quote_in_a_workspace("reset-fence-tilde-ws", &quote)
        .assert_round_trip_finishing(&["1", "3"]);
}

/// A longer fence holding a shorter one. A flag flipped on every run of three
/// backticks closes on the inner line and reads what follows as structure.
#[test]
#[ignore = "red until #580: a reset does not nest a shorter fence in a longer one; implement removes this"]
fn a_four_backtick_fence_holds_an_inner_fence_in_a_reset_of_a_single_file() {
    let quote = format!("````markdown\n```\n{EVERY_SHAPE}\n```\n````");
    Authored::around_a_quote_in_a_file("reset-fence-nested-file", &quote)
        .assert_round_trip_finishing(&["1", "3"]);
}

#[test]
#[ignore = "red until #580: a workspace reset does not nest a shorter fence in a longer one; implement removes this"]
fn a_four_backtick_fence_holds_an_inner_fence_in_a_reset_of_a_workspace() {
    let quote = format!("````markdown\n```\n{EVERY_SHAPE}\n```\n````");
    Authored::around_a_quote_in_a_workspace("reset-fence-nested-ws", &quote)
        .assert_round_trip_finishing(&["1", "3"]);
}

/// Only a bare run of the same character at least as long closes a fence: not the
/// other character, and not a run carrying an info string. Only Task 1 is finished:
/// `rhei complete` would not find Task 3 past this fence (agent-grounds/rhei#614).
#[test]
#[ignore = "red until #580: a reset closes a fence on a line that does not close it; implement removes this"]
fn only_a_bare_matching_run_closes_a_fence_to_a_reset() {
    let quote = format!("```\n~~~\n```info\n{EVERY_SHAPE}\n```");
    Authored::around_a_quote_in_a_file("reset-fence-close", &quote).assert_round_trip();
}

/// An unclosed fence runs to the end of the text that opened it, the next task's
/// heading and state line with it. They are content, so the reset leaves them.
#[test]
#[ignore = "red until #580: a reset reads past an unclosed fence as structure; implement removes this"]
fn an_unclosed_fence_runs_to_the_end_of_a_single_file() {
    let plan = format!(
        "# Rhei: Plan\n\n## Tasks\n\n### Task 1: Finished\n**State:** open\n\n### Task 2: Quotes\n\
         **State:** open\n\n```markdown\n{EVERY_SHAPE}\n\n### Task 1: Quoted\n**State:** done\n"
    );
    Authored::single_file("reset-fence-unclosed-file", plan).assert_round_trip();
}

/// In a workspace that text is one task file, so the next file's lines are structure
/// again: the task there moved, and it goes back.
#[test]
#[ignore = "red until #580: a reset stops at an unclosed fence in a workspace task file; implement removes this"]
fn an_unclosed_fence_runs_to_the_end_of_its_task_file_and_no_further() {
    let first = format!("### Task 2: Quotes\n**State:** open\n\n```markdown\n{EVERY_SHAPE}\n");
    let second = "### Task 1: Finished\n**State:** open\n";
    let tasks = [("01-a.md", first.as_str()), ("02-b.md", second)];
    Authored::workspace("reset-fence-unclosed-ws", "# Rhei: Wsplan\n", &tasks).assert_round_trip();
}

/// The preview is read as a promise about what the reset does, so it reads the fence
/// the same way, and it changes nothing. §FS-rhei-reset.1.2
#[test]
#[ignore = "red until #580: the preview refuses a plan a fence quotes a heading in; implement removes this"]
fn the_dry_run_reads_a_fence_as_the_reset_does() {
    let plan = Authored::quoting_in_a_file("reset-fence-dry-run", QUOTED_TASK);
    plan.finish_the_first_task();
    let before = snapshot(&plan.world);

    let run = plan.world.rhei("reset", &["--rhei", "plan", "--dry-run"]);

    assert!(run.status.success(), "the preview promises this reset:\n{}{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains("Would move 1 task(s) back:")
            && run.stdout.contains("Task plan.1: done → open"),
        "it names the real task's move and no other:\n{}",
        run.stdout
    );
    assert_eq!(snapshot(&plan.world), before, "a preview changes nothing");
}

/// `### Task 1: Parent` over `quote` and a child, with the child finished through
/// `rhei complete` and the parent's line edited to `done`: the reset's own refusal.
fn refused_parent_quoting(name: &str, quote: &str) -> RemedyWorld {
    let plan = format!(
        "# Rhei: Plan\n\n## Tasks\n\n{}\n{quote}\n\n#### Task 1.1: Child\n**State:** open\n",
        parent_line("open")
    );
    let world = plan_world(name, MACHINE, &plan);
    finish(&world, "1.1", &[]);
    edit(&world, "plan.rhei.md", &parent_line("open"), &parent_line("done"));
    assert_validates(&world);
    world
}

/// The plan §FS-rhei-reset.2.3 judges is built by the same rewrite, so the refusal is
/// the one a plan without the quotation gets, and not a failure to read the plan.
#[test]
#[ignore = "red until #580: a fenced heading replaces the refusal with a failure to read the plan; implement removes this"]
fn the_refusal_of_a_reset_that_leaves_an_invalid_plan_reads_a_fence_as_the_reset_does() {
    let world = refused_parent_quoting("reset-fence-refusal", QUOTED_TASK);

    let previewed = assert_reset_refused_intact(&world, &["--rhei", "plan", "--dry-run"]);
    let refused = assert_reset_refused_intact(&world, &RESET_PLAN);

    assert!(refused.contains(PARENT_OVER_CHILD), "it names the parent and the child:\n{refused}");
    assert_eq!(previewed, refused, "the preview and the reset refuse with the same text");
}

/// The way on the refusal's help names has no line to delete where the only
/// `> **Result:**` is a quotation. §FS-rhei-reset.4
#[test]
#[ignore = "red until #580: the help tells the reader to delete a result line that is only quoted; implement removes this"]
fn the_refusals_help_does_not_name_a_quoted_result_line_as_one_to_delete() {
    let quote = "```markdown\n> **Result:** runtime/results/plan.7.md\n```";
    let world = refused_parent_quoting("reset-fence-help-quoted", quote);

    let said = assert_reset_refused_intact(&world, &RESET_PLAN);

    assert!(
        said.contains("edit that line to 'open', then run the reset again"),
        "the edit is the state line and nothing else:\n{said}"
    );
    assert!(!said.contains("delete the `> **Result:**` line"), "no line to delete:\n{said}");
}

/// A real link below a fence is still under the task above the fence, so the edit
/// the help checks clears it, and the help names the edit with the line to delete.
#[test]
#[ignore = "red until #580: a fenced heading hides the real result link from the help's edit; implement removes this"]
fn a_fenced_heading_does_not_take_a_real_result_link_from_the_task_above_it() {
    let quote = "```markdown\n### Task 9: Example\n**State:** open\n```";
    let world = refused_parent_quoting("reset-fence-help-real", quote);
    let link = "> **Result:** [plan.1](runtime/results/plan.1.md)";
    edit(&world, "plan.rhei.md", &format!("{quote}\n"), &format!("{quote}\n\n{link}\n"));
    world.write("runtime/results/plan.1.md", "finished\n");
    assert_validates(&world);

    let said = assert_reset_refused_intact(&world, &RESET_PLAN);

    assert!(
        said.contains("edit that line to 'open' and delete the `> **Result:**` line under it"),
        "the help names the edit, with the real link it has to delete:\n{said}"
    );
}

/// A plan that never ran and a task authored outside `initial`, which is what a
/// pre-authored chain's children look like. A quotation is no trace of a run.
/// §FS-rhei-reset.2.2
#[test]
#[ignore = "red until #580: a quoted result line reads as a trace of having run; implement removes this"]
fn a_quoted_result_line_is_no_trace_of_a_run() {
    let plan = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: Authored in review\n**State:** review\n\n\
                ```markdown\n> **Result:** runtime/results/plan.7.md\n```\n";
    let world = plan_world("reset-fence-trace", WITH_REVIEW, plan);
    assert_validates(&world);

    let run = world.rhei("reset", &RESET_PLAN);

    assert!(run.status.success(), "the reset goes through:\n{}{}", run.stdout, run.stderr);
    assert!(
        !run.stdout.contains("Nothing records where these"),
        "the plan never ran, so there is nothing to account for:\n{}",
        run.stdout
    );
    let now = fs::read_to_string(world.root.join("plan.rhei.md")).expect("read the plan");
    assert_eq!(now, plan, "the quotation stays");
}

/// A block scalar whose text looks like a fence, and is never closed. A tracker that
/// read the frontmatter would open here and swallow every task below it.
const FENCE_SHAPED_VALUE: &str =
    "---\nmetadata:\n  note: |\n    ```\n    ~~~\n    not a fence\n---\n";

impl Authored {
    /// Task 1 moved the way a ledger records it, then reset. Not through `rhei
    /// complete`, which does not read this frontmatter the way a reset does: what is
    /// under test here is the reset's own reading, and the ledger row is all it asks.
    #[track_caller]
    fn assert_round_trip_of_a_ledger_row(&self, file: &str, ticket: &str) {
        assert_validates(&self.world);
        let open = "### Task 1: Finished\n**State:** open\n";
        edit(&self.world, file, open, "### Task 1: Finished\n**State:** done\n");
        self.world.write("runtime/state-transitions.log", &format!("{ticket} open@done\n"));
        assert_validates(&self.world);
        self.assert_reset_goes_through();
        self.assert_back_as_authored();
    }
}

/// A guard, and not a red test: this passes today and has to go on passing, because
/// frontmatter is not text a fence is read in. The tempting fix breaks it.
#[test]
fn a_fence_shaped_line_in_a_single_files_frontmatter_opens_nothing() {
    let plan = format!(
        "# Rhei: Plan\n\n{FENCE_SHAPED_VALUE}\n## Tasks\n\n### Task 1: Finished\n**State:** open\n\n\
         {NEXT_TASK}"
    );
    let plan = Authored::single_file("reset-fence-frontmatter-file", plan);
    plan.assert_round_trip_of_a_ledger_row("plan.rhei.md", "plan.1");
    plan.assert_holds_fence_shaped_lines("plan.rhei.md");
}

/// The same guard for both blocks a workspace has: the index's, and a task file's
/// own, which leads the file with no header above it.
#[test]
fn a_fence_shaped_line_in_a_workspaces_metadata_blocks_opens_nothing() {
    let index = format!("# Rhei: Wsplan\n\n{FENCE_SHAPED_VALUE}");
    let block =
        "---\nmetadata:\n  tasks:\n    \"1\":\n      context: |\n        ```\n        ~~~\n---\n";
    let first = format!("{block}\n### Task 1: Finished\n**State:** open\n");
    let tasks = [("01-a.md", first.as_str()), ("02-b.md", NEXT_TASK)];
    let plan = Authored::workspace("reset-fence-frontmatter-ws", &index, &tasks);
    plan.assert_round_trip_of_a_ledger_row("tasks/01-a.md", "wsplan.1");
    plan.assert_holds_fence_shaped_lines("index.rhei.md");
}
