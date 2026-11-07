    // What a dependency cycle's error names and in what order. Included into
    // `mod tests` in `state_machine/mod.rs`, so the indentation is the module's.

    // §FS-rhei-plan-language.3.3

    /// A plan of top-level tasks, each `(id, the ids it waits on)`.
    fn plan_of(tasks: &[(u32, &[u32])]) -> Rhei {
        let mut input = String::from("# Rhei: Ex\n## Tasks\n");
        for (id, priors) in tasks {
            input.push_str(&format!("\n### Task {id}: T\n**State:** pending\n"));
            if !priors.is_empty() {
                let names: Vec<String> = priors.iter().map(|p| format!("Task {p}")).collect();
                input.push_str(&format!("**Prior:** {}\n", names.join(", ")));
            }
        }
        parse(&input).expect("parse ok")
    }

    /// The cycle errors a plan gets, whole and in order.
    fn cycle_errors(tasks: &[(u32, &[u32])]) -> Vec<String> {
        validate_with_machine(&plan_of(tasks), &sample_machine())
            .errors
            .into_iter()
            .filter(|error| error.contains("Circular dependency"))
            .collect()
    }

    /// The text §FS-rhei-plan-language.3.3 gives a cycle of these tasks.
    fn cycle_text(names: &[&str]) -> String {
        let quoted: Vec<String> = names.iter().map(|name| format!("{name:?}")).collect();
        format!("Circular dependency detected among tasks: [{}]", quoted.join(", "))
    }

    /// Ten tasks in one ring. Text order puts 10 after 1, and any other order of
    /// the ten fails: hash order lands on this one about once in 3.6 million.
    #[test]
    #[ignore = "red until #582: the error lists a ring of ten in hash order; implement removes this"]
    fn a_long_cycle_lists_its_tasks_sorted_as_text() {
        let tasks: [(u32, &[u32]); 10] = [
            (1, &[2]),
            (2, &[3]),
            (3, &[4]),
            (4, &[5]),
            (5, &[6]),
            (6, &[7]),
            (7, &[8]),
            (8, &[9]),
            (9, &[10]),
            (10, &[1]),
        ];
        let names = ["1", "10", "2", "3", "4", "5", "6", "7", "8", "9"];
        assert_eq!(cycle_errors(&tasks), vec![cycle_text(&names)]);
    }

    /// Task 3 waits on the cycle and is on none. Today's list is what Kahn's
    /// algorithm could not drain, so it names 3, in any order.
    #[test]
    #[ignore = "red until #582: the error names a task downstream of the cycle; implement removes this"]
    fn a_task_downstream_of_a_cycle_is_not_named() {
        let tasks: [(u32, &[u32]); 4] = [(1, &[2]), (2, &[1]), (3, &[1]), (4, &[3])];
        assert_eq!(cycle_errors(&tasks), vec![cycle_text(&["1", "2"])]);
    }

    /// One error per cycle is what lets a standing cycle keep its text when
    /// another appears, and the errors come in the order of their first names.
    #[test]
    #[ignore = "red until #582: two disjoint cycles share one error; implement removes this"]
    fn two_disjoint_cycles_are_two_errors() {
        let tasks: [(u32, &[u32]); 4] = [(1, &[2]), (2, &[1]), (3, &[4]), (4, &[3])];
        assert_eq!(cycle_errors(&tasks), vec![cycle_text(&["1", "2"]), cycle_text(&["3", "4"])]);
    }

    /// Task 3 is reached from one cycle and reaches the other, so a filter that
    /// only drops what no cycle waits on would still name it.
    #[test]
    #[ignore = "red until #582: the error names a bridge between two cycles; implement removes this"]
    fn a_bridge_between_two_cycles_is_named_by_neither() {
        let tasks: [(u32, &[u32]); 5] =
            [(1, &[2]), (2, &[1]), (3, &[1]), (4, &[3, 5]), (5, &[4])];
        assert_eq!(cycle_errors(&tasks), vec![cycle_text(&["1", "2"]), cycle_text(&["4", "5"])]);
    }

    /// Two rings through task 5 are one fault, with nine names in one error.
    /// Hash order lands on this list about once in 360 thousand.
    #[test]
    #[ignore = "red until #582: the error lists two joined rings in hash order; implement removes this"]
    fn cycles_that_share_a_task_are_one_error() {
        let tasks: [(u32, &[u32]); 9] = [
            (1, &[2]),
            (2, &[3]),
            (3, &[4]),
            (4, &[5]),
            (5, &[1, 6]),
            (6, &[7]),
            (7, &[8]),
            (8, &[9]),
            (9, &[5]),
        ];
        let names = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
        assert_eq!(cycle_errors(&tasks), vec![cycle_text(&names)]);
    }

    /// A guard rather than a red test: it passes today, and it must survive a
    /// search that reports only components of two or more tasks.
    #[test]
    fn a_task_that_is_its_own_prior_is_a_cycle_of_one() {
        let tasks: [(u32, &[u32]); 1] = [(1, &[1])];
        assert_eq!(cycle_errors(&tasks), vec![cycle_text(&["1"])]);
    }

    /// The same self-edge with something waiting on it, which today names both.
    #[test]
    #[ignore = "red until #582: the error names a task waiting on a self-edge; implement removes this"]
    fn a_task_waiting_on_a_self_edge_is_not_named() {
        let tasks: [(u32, &[u32]); 2] = [(1, &[1]), (2, &[1])];
        assert_eq!(cycle_errors(&tasks), vec![cycle_text(&["1"])]);
    }

    /// A guard: a task upstream of a cycle drains in Kahn's pass today and is not
    /// named, and a plan with no cycle gets no such error.
    #[test]
    fn a_task_a_cycle_waits_on_is_not_named() {
        let upstream: [(u32, &[u32]); 3] = [(1, &[]), (2, &[1, 3]), (3, &[2])];
        let errors = cycle_errors(&upstream);
        assert_eq!(errors.len(), 1, "one cycle: {errors:?}");
        assert!(!errors[0].contains("\"1\""), "task 1 is on no cycle: {errors:?}");
        let chain: [(u32, &[u32]); 2] = [(1, &[]), (2, &[1])];
        assert_eq!(cycle_errors(&chain), Vec::<String>::new());
    }
