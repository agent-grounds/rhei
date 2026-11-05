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
        let edited = strip_result_links_under(raw, |id| id == Some("1"));
        assert!(!edited.contains("[plan.1]("), "{edited}");
        assert!(edited.contains("[plan.1.1]("), "{edited}");
        assert_eq!(strip_result_links_under(raw, |_| true), strip_result_links(raw));
    }
}
