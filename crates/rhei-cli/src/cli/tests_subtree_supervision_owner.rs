    // Owner selection is tested apart from checkpoint emission: the event
    // filter deliberately is not an input to this rule.

    // §AR-source-file-size.3 §FS-rhei-supervision.2.2

    fn owner_in_plan(plan: &rhei_core::ast::Rhei, task_id: &str) -> Option<TaskId> {
        let machine = scope_machine();
        let target = parse_task_id(task_id);
        let ancestors: Vec<rhei_core::ast::Task> =
            ancestor_chain(&plan.tasks, &target).into_iter().cloned().collect();
        nearest_in_scope_supervising_owner(&machine, &ancestors).map(|task| task.id.clone())
    }

    fn owner_plan(outer_state: &str, middle_state: &str) -> rhei_core::ast::Rhei {
        rhei_core::parse(&format!(
            "# Rhei: Owners\n---\nstructure:\n  maxLevels: 4\n---\n\n## Tasks\n\n\
             ### Task 1: Outer\n**State:** {outer_state}\n\n\
             #### Task 1.1: Middle\n**State:** {middle_state}\n\n\
             ##### Task 1.1.1: Inner\n**State:** review\n\n\
             ###### Task 1.1.1.1: Leaf\n**State:** review\n"
        ))
        .expect("parse owner plan")
    }

    /// §FS-rhei-supervision.2.2: the first in-scope supervising ancestor owns
    /// the move; an in-scope outer supervisor never also receives it.
    #[test]
    fn nearest_nested_supervisor_is_the_owner() {
        let plan = owner_plan("supervising", "watching");
        assert_eq!(owner_in_plan(&plan, "1.1.1"), Some(parse_task_id("1.1")));
    }

    /// §FS-rhei-supervision.2.2: a `child-*` supervisor excludes a grandchild,
    /// so ownership climbs to the outer `descendant-*` supervisor.
    #[test]
    fn child_scope_exclusion_climbs_to_an_outer_owner() {
        let plan = owner_plan("supervising", "watching");
        assert_eq!(owner_in_plan(&plan, "1.1.1.1"), Some(parse_task_id("1")));
    }

    /// §FS-rhei-supervision.2.2: event matching happens after ownership; the
    /// `child-terminal` owner remains selected for a non-terminal child hop.
    #[test]
    fn event_filter_miss_does_not_change_the_owner() {
        let plan = owner_plan("supervising", "watching");
        assert_eq!(owner_in_plan(&plan, "1.1.1"), Some(parse_task_id("1.1")));
    }

    /// §FS-rhei-supervision.2.2: without an in-scope supervising ancestor the
    /// transition has no supervision owner.
    #[test]
    fn task_without_a_supervising_ancestor_has_no_owner() {
        let plan = owner_plan("review", "review");
        assert_eq!(owner_in_plan(&plan, "1.1.1.1"), None);
    }
