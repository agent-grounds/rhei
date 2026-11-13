// The three rules a `--task` reset rests on that its end-to-end tests reach only
// through whole runs: the closure and the edge each ticket is listed by, the
// stripping that keeps a kept ticket's lines in a file it shares, and the
// longest id a prefix-matched file belongs to.

// §AR-source-file-size.3 §FS-rhei-reset.2.5

mod reset_task_scope {
    use super::super::*;

    const MACHINE: &str = "name: scope
version: 1
states:
  pending: { initial: true, description: Open }
  supervising: { description: Supervise, execute_on: child-terminal, agent: pi }
  completed: { final: true, description: Done }
transitions:
  - { from: pending, to: completed, description: Done }
  - { from: supervising, to: completed, description: Done }
  - { from: supervising, to: supervising, description: Released }
";

    /// `x.1` finished over two children, `x.2` after it with a child of its own,
    /// `x.3` apart, and `x.4` a supervisor over a chain.
    const PLAN: &str = "# Rhei: X

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: parent
**State:** completed

#### Task 1.1: first child
**State:** completed

#### Task 1.2: second child
**State:** completed

### Task 2: after the parent
**State:** pending
**Prior:** Task 1

#### Task 2.1: its child
**State:** pending

### Task 3: apart
**State:** pending

### Task 4: supervisor
**State:** supervising

#### Task 4.1: implement
**State:** completed

#### Task 4.2: review
**State:** completed
**Prior:** Task 4.1
";

    fn machines() -> rhei_validator::MachineSet {
        let machine = rhei_validator::StateMachine::from_yaml_str(MACHINE).expect("machine");
        rhei_validator::MachineSet { default: machine, per_rhei: BTreeMap::new() }
    }

    fn plan(dir: &tempfile::TempDir, text: &str) -> (PathBuf, LoadedPlan) {
        let path = dir.path().join("x.rhei.md");
        fs::write(&path, text).expect("write the plan");
        let loaded = load_plan(&path).expect("load the plan");
        (path, loaded)
    }

    fn closure(loaded: &LoadedPlan, named: &[&str]) -> TicketScope {
        let named: Vec<String> = named.iter().map(|id| id.to_string()).collect();
        TicketScope::close(loaded, &named, None, &machines())
    }

    fn edges(scope: &TicketScope) -> Vec<(&str, Option<String>)> {
        scope.reached.iter().map(|(id, edge)| (id.as_str(), edge.label())).collect()
    }

    /// A finished parent is taken alone and listed by the ticket under it, while its
    /// dependent comes with its child; the sibling and the unrelated ticket stay.
    /// §FS-rhei-reset.2.5.1 §FS-rhei-reset.2.5.2
    #[test]
    fn the_closure_takes_a_finished_parent_alone_and_lists_each_ticket_by_its_edge() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_, loaded) = plan(&dir, PLAN);
        let scope = closure(&loaded, &["x.1.2"]);
        assert_eq!(
            edges(&scope),
            vec![
                ("x.1", Some("(finished parent of x.1.2)".to_string())),
                ("x.1.2", None),
                ("x.2", Some("(prior: x.1)".to_string())),
                ("x.2.1", Some("(child of x.2)".to_string())),
            ]
        );
    }

    /// A named ticket brings its descendants and dependents, never its priors.
    /// §FS-rhei-reset.2.5.1
    #[test]
    fn a_named_tickets_priors_stay_out_of_the_closure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_, loaded) = plan(&dir, PLAN);
        let scope = closure(&loaded, &["x.2"]);
        assert_eq!(edges(&scope), vec![("x.2", None), ("x.2.1", Some("(child of x.2)".to_string()))]);
        assert!(scope.supervisors.is_empty());
    }

    /// A ticket first reached as a finished ancestor and then by a dependent edge brings
    /// its descendants after all, and keeps the edge that reached it first.
    /// §FS-rhei-reset.2.5.1
    #[test]
    fn a_finished_ancestor_reached_again_as_a_dependent_brings_its_children() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = PLAN.replacen("### Task 1: parent\n**State:** completed\n", "### Task 1: parent\n**State:** completed\n**Prior:** Task 3\n", 1);
        let (_, loaded) = plan(&dir, &text);
        let scope = closure(&loaded, &["x.1.2", "x.3"]);
        assert_eq!(scope.edge("x.1"), Some(&ScopeEdge::FinishedParentOf("x.1.2".to_string())));
        assert_eq!(scope.edge("x.1.1"), Some(&ScopeEdge::ChildOf("x.1".to_string())));
    }

    /// The supervisor a scope leaves out is recorded with the tickets in scope it
    /// owns, in plan order; one the scope takes is not. §FS-rhei-reset.2.5.3
    #[test]
    fn a_supervisor_the_scope_leaves_out_is_recorded_with_the_tickets_it_owns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_, loaded) = plan(&dir, PLAN);
        let scope = closure(&loaded, &["x.4.1"]);
        assert_eq!(
            scope.supervisors,
            BTreeMap::from([("x.4".to_string(), vec!["x.4.1".to_string(), "x.4.2".to_string()])])
        );
        assert!(closure(&loaded, &["x.4"]).supervisors.is_empty());
    }

    /// A ticket scope strips the claims and result links under its own tickets'
    /// headings only; a reset of whole rheis strips every one. §FS-rhei-reset.2.5
    /// §FS-rhei-reset.2.4
    #[test]
    fn a_ticket_scope_strips_runtime_lines_under_its_own_headings_only() {
        let raw = "# Rhei: X\n\n## Tasks\n\n### Task 1: kept\n**State:** completed\n**Assignee:** a\n\n\
                   > **Result:** [runtime/results/x.1.md](runtime/results/x.1.md)\n\n\
                   ### Task 2: reset\n**State:** completed\n**Assignee:** b\n\n\
                   > **Result:** [runtime/results/x.2.md](runtime/results/x.2.md)\n";
        let authored = BTreeMap::from([("2".to_string(), "pending".to_string())]);
        let tickets = TicketScope {
            reached: BTreeMap::from([("x.2".to_string(), ScopeEdge::Named)]),
            resolved_in: None,
            supervisors: BTreeMap::new(),
        };
        let scope = ResetScope::Tickets(&tickets);
        let text = reset_file_text(raw, &authored, PlanFileKind::Plan, reset_strips_under(scope, &authored))
            .expect("rewrite");
        assert!(text.contains("**Assignee:** a\n\n> **Result:** [runtime/results/x.1.md]"), "{text}");
        assert!(text.contains("### Task 2: reset\n**State:** pending\n"), "{text}");
        assert!(!text.contains("**Assignee:** b") && !text.contains("x.2.md"), "{text}");

        let whole: RheiScope = None;
        let all = reset_file_text(raw, &authored, PlanFileKind::Plan, reset_strips_under((&whole).into(), &authored))
            .expect("rewrite");
        assert!(!all.contains("**Assignee:**") && !all.contains("> **Result:**"), "{all}");
        assert_eq!(strip_assignee_lines(raw, PlanFileKind::Plan), strip_assignee_lines_under(raw, PlanFileKind::Plan, |_| true));
    }

    /// A file a shorter id's prefix matches belongs to the longest id whose prefix
    /// matches it, so `x.build` leaves `x.build-docs`'s. §FS-rhei-reset.2.1
    #[test]
    fn a_prefix_matched_file_goes_to_the_longest_id_whose_prefix_it_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, loaded) = plan(
            &dir,
            "# Rhei: X\n\n## Tasks\n\n### Task build: build\n**State:** pending\n\n\
             ### Task build-docs: docs\n**State:** pending\n",
        );
        let logs = dir.path().join("runtime/logs");
        fs::create_dir_all(&logs).expect("logs");
        for name in ["task-x.build-pending.log", "task-x.build-docs-pending.log"] {
            fs::write(logs.join(name), "log\n").expect("a log");
        }

        let owners = PrefixOwners::index(&loaded, &path, &machines());
        let short = owners.entries(&logs, "task-x.build-").expect("list");
        assert_eq!(short, vec![logs.join("task-x.build-pending.log")]);
        let long = owners.entries(&logs, "task-x.build-docs-").expect("list");
        assert_eq!(long, vec![logs.join("task-x.build-docs-pending.log")]);
    }

    /// The kept line names five tickets and counts the rest. §FS-rhei-reset.4
    #[test]
    fn the_kept_line_names_five_tickets_and_counts_the_rest() {
        let kept: Vec<String> = (2..=8).map(|n| format!("z.{n}")).collect();
        assert_eq!(
            kept_tickets_line(&kept).as_deref(),
            Some("Kept 7 task(s) as they stand, with their results and logs: z.2, z.3, z.4, z.5, z.6 and 2 more")
        );
        assert_eq!(kept_tickets_line(&kept[..1]).as_deref(), Some("Kept 1 task(s) as they stand, with their results and logs: z.2"));
        assert_eq!(kept_tickets_line(&[]), None);
    }
}
