// The two text rules a refused reset's help rests on: which tasks an error names,
// and which result links an edit under named headings drops.
//
// Its own part because the end-to-end tests reach both only through whole plans,
// where a task whose id extends another's never shows up.

// §AR-source-file-size.3 §FS-rhei-reset.2.3

mod reset_refusal {
    use super::super::*;

    /// A task is named by its label ending where its id ends, so `plan.1` is not
    /// named by an error about `plan.1.1` or `plan.10` alone. §FS-rhei-reset.4
    #[test]
    fn an_error_names_a_task_only_where_its_id_ends() {
        let tree = "Task plan.1 is in terminal state 'done' but descendant Task plan.1.1 \
                    ('Child') is in non-terminal state 'open'";
        assert!(error_names(tree, "Task plan.1"));
        assert!(error_names(tree, "Task plan.1.1"));
        assert!(!error_names("Task plan.1.1 ('Child') is open", "Task plan.1"));
        assert!(!error_names("Task plan.10 is open", "Task plan.1"));
        assert!(error_names("it ends at Task plan.1.", "Task plan.1"));
    }

    /// A refusal whose help names no edit and no moved task still ends on a help line.
    /// No plan `rhei validate` accepts was found to reach it. §FS-rhei-reset.4
    #[test]
    fn a_refusal_with_no_way_on_named_says_so_rather_than_print_an_empty_help() {
        let refusal = ResetRefusal {
            errors: vec!["Task plan.1 is in terminal state 'done'".to_string()],
            undeclared: Vec::new(),
            help: Vec::new(),
        };
        let help = refusal.report().help().map(|help| help.to_string());
        assert_eq!(
            help.as_deref(),
            Some(
                "no edit to a **State:** line is known to let this reset through: fix what the \
                 errors above name, then run the reset again"
            )
        );
    }

    /// The edit the help names drops the result link under the task it edits, and
    /// keeps every other. §FS-rhei-reset.2.3
    #[test]
    fn result_links_are_dropped_only_under_the_headings_asked_for() {
        let raw = "### Task 1: Parent\n**State:** done\n\n> **Result:** [plan.1](runtime/results/plan.1.md)\n\n\
                   #### Task 1.1: Child\n**State:** done\n\n> **Result:** [plan.1.1](runtime/results/plan.1.1.md)\n";
        let edited = strip_result_links_under(raw, PlanFileKind::Plan, |id| id == Some("1"));
        assert!(!edited.contains("[plan.1]("), "{edited}");
        assert!(edited.contains("[plan.1.1]("), "{edited}");
        assert_eq!(
            strip_result_links_under(raw, PlanFileKind::Plan, |_| true),
            strip_result_links(raw, PlanFileKind::Plan)
        );
    }

    /// A task file has no header, so a `# Rhei:` line in it opens no frontmatter,
    /// above its first task or below one: every task is still read, and its state
    /// goes back. §FS-rhei-reset.2.4
    #[test]
    fn a_header_shaped_line_in_a_task_file_opens_no_frontmatter() {
        let below = "### Task 1: Quotes\n**State:** open\n\n# Rhei: Quoted\n\n---\n\n\
                     ### Task 2: Next\n**State:** done\n";
        let above = "# Rhei: Notes\n\n---\n\n### Task 2: Next\n**State:** done\n";
        let authored = BTreeMap::from([("2".to_string(), "open".to_string())]);
        for raw in [below, above] {
            let rewritten = rewrite_states_to_authored(raw, &authored, PlanFileKind::TaskFile)
                .expect("rewrite states");
            assert_eq!(rewritten, raw.replace("**State:** done", "**State:** open"));
        }
    }

    /// The opening line of the fence a task's text ends in, read as the parser reads
    /// it: only that task's, and never one a frontmatter value looks like.
    /// §FS-rhei-complete.3.3
    #[test]
    fn a_tasks_final_fence_is_cut_at_its_opening_line() {
        let text = "### Task 1: A\n**State:** open\n\n~~~~\n~~~\n### Task 2: Quoted\n";
        let lines: Vec<&str> = text.lines().collect();
        let (above, fence) = split_at_a_tasks_final_fence(&lines, PlanFileKind::TaskFile, "1");
        assert_eq!(above.len(), 3);
        assert_eq!(fence.first(), Some(&"~~~~"));
        let (_, fence) = split_at_a_tasks_final_fence(&lines, PlanFileKind::TaskFile, "2");
        assert!(fence.is_empty(), "the quoted heading is no task: {fence:?}");

        let text = "---\nnote: |\n  ```\n---\n### Task 1: A\n**State:** open\n";
        let lines: Vec<&str> = text.lines().collect();
        let (_, fence) = split_at_a_tasks_final_fence(&lines, PlanFileKind::TaskFile, "1");
        assert!(fence.is_empty(), "frontmatter opens no fence: {fence:?}");

        let closed = "### Task 1: A\n**State:** open\n\n```\nx\n```\n";
        let lines: Vec<&str> = closed.lines().collect();
        let (_, fence) = split_at_a_tasks_final_fence(&lines, PlanFileKind::Plan, "1");
        assert!(fence.is_empty(), "a closed fence leaves the task's end outside: {fence:?}");
    }

    /// A task whose link already stands above its final fence has that one refreshed.
    /// A machine has no edge out of a final state, and validation refuses a link on an
    /// open task, so no command reaches this path twice: it is pinned here.
    /// §FS-rhei-complete.3.3
    #[test]
    fn a_second_completion_refreshes_the_link_above_a_tasks_final_fence() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.rhei.md");
        let text = "### Task 1: A\n**State:** done\n\nBody\n\n```text\nopen\n";
        fs::write(&path, text).expect("write");
        let link = "runtime/results/plan.1.md";
        rewrite_task_completion(&path, PlanFileKind::Plan, "1", "plan.1", link, true).expect("first");
        let first = fs::read_to_string(&path).expect("read");
        assert_eq!(
            first,
            format!("### Task 1: A\n**State:** done\n\nBody\n\n> **Result:** [plan.1]({link})\n\n```text\nopen\n")
        );
        rewrite_task_completion(&path, PlanFileKind::Plan, "1", "plan.1", link, true).expect("second");
        assert_eq!(fs::read_to_string(&path).expect("read"), first);
    }

    /// The forced path leaves the one fence in place too, and a reopening finds the link
    /// it wrote. §FS-rhei-complete.3.3
    #[test]
    fn a_forced_result_link_goes_above_a_tasks_final_fence() {
        let raw = "### Task 1: A\n**State:** done\n\nBody\n\n```text\nopen\n";
        let link = "> **Result:** [p.1](runtime/results/p.1.md)";
        let forced = forced_result_link(raw, PlanFileKind::TaskFile, "1", "p.1", true);
        assert_eq!(
            forced,
            format!("### Task 1: A\n**State:** done\n\nBody\n\n{link}\n\n```text\nopen\n")
        );
        let reopened = forced_result_link(&forced, PlanFileKind::TaskFile, "1", "p.1", false);
        assert!(!reopened.contains("Result:"), "{reopened}");
        assert!(reopened.ends_with("```text\nopen\n"), "{reopened}");
    }

    /// A plan document is read as one, and any file the load read tickets from under a
    /// directory it takes task files from is a task file, whatever it is named. The load
    /// decides, not the path. §FS-rhei-reset.2.4 §FS-rhei-plan-language.2.1
    #[test]
    fn a_files_kind_comes_from_where_the_loader_reads_it() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let root = &dir.path().join("ws");
        fs::create_dir(root).expect("workspace dir");
        fs::write(root.join("index.rhei.md"), "# Rhei: W\n").expect("write index");
        fs::create_dir(root.join("tasks")).expect("tasks dir");
        fs::write(root.join("tasks/01.md"), "### Task 1: A\n\n**State:** open\n").expect("task");
        fs::write(root.join("tasks/01.rhei.md"), "### Task 2: B\n\n**State:** open\n").expect("task");
        let loaded = load_plan(root).expect("load workspace");
        assert_eq!(loaded.document_role(&root.join("index.rhei.md")), PlanFileKind::Plan);
        assert_eq!(loaded.document_role(Path::new("plan.rhei.md")), PlanFileKind::Plan);
        assert_eq!(loaded.document_role(&root.join("tasks/01.md")), PlanFileKind::TaskFile);
        assert_eq!(loaded.document_role(&root.join("tasks/01.rhei.md")), PlanFileKind::TaskFile);
    }
}
