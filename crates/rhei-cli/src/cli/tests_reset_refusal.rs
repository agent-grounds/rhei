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
