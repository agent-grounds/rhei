    fn sample_machine_with_models() -> StateMachine {
        StateMachine::from_yaml_str(
            r#"
name: model-overrides
version: 1.0
models:
  - gpt-5
  - claude-opus-4-7
states:
  pending:
    model: gpt-5
    agent: codex
  completed:
    final: true
transitions:
  - from: pending
    to: completed
"#,
        )
        .expect("states load")
    }

    #[test]
    fn reports_missing_named_dependency() {
        let input = r#"# Rhei: Example
## Tasks

### Task build: Build step
**State:** pending
**Prior:** Task deploy

### Task test: Test step
**State:** in-progress
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected missing named dependency error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task build depends on missing Task deploy"),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    /// Build a merged-project-shaped task (qualified id, offset 1).
    fn qualified_task(rhei_id: &str, local: u32, state: &str, prior: Vec<TaskId>) -> Task {
        use crate::ast::TaskIdSegment;
        Task {
            id: TaskId::from_segments(vec![
                TaskIdSegment::Named(rhei_id.to_string()),
                TaskIdSegment::Number(local),
            ]),
            profile_depth_offset: 1,
            kind: "task".to_string(),
            title: format!("{rhei_id} {local}"),
            state: state.to_string(),
            prior_kinds: vec![None; prior.len()],
            prior,
            inherits: None,
            provides: Vec::new(),
            consumes: Vec::new(),
            excludes: Vec::new(),
            assignee: None,
            model: None,
            target: None,
            content: String::new(),
            children: Vec::new(),
        }
    }

    // §DA-per-rhei-state-machines: each ticket validates under its owning
    // rhei's machine, and a cross-rhei prior's terminal-ness is judged under
    // the *target's* machine.
    #[test]
    fn machine_set_dispatches_per_owning_rhei() {
        let default_machine = sample_machine(); // pending / in-progress / completed
        let review_machine = StateMachine::from_yaml_str(
            r#"
name: review-loop
version: 1.0
states:
  draft: { description: "writing" }
  done: { final: true, description: "reviewed" }
transitions:
  - from: draft
    to: done
"#,
        )
        .expect("review machine loads");

        let rhei = Rhei {
            title: "Project".to_string(),
            states: default_machine.name.clone(),
            states_declared: true,
            structure: Default::default(),
            metadata: None,
            content_sections: Vec::new(),
            tasks: vec![
                qualified_task("plain", 1, "completed", Vec::new()),
                // `draft` exists only in review-loop; `done` waits on plain.1
                // judged under the *default* machine.
                qualified_task(
                    "review",
                    1,
                    "done",
                    vec![TaskId::from_segments(vec![
                        crate::ast::TaskIdSegment::Named("plain".to_string()),
                        crate::ast::TaskIdSegment::Number(1),
                    ])],
                ),
                qualified_task("review", 2, "draft", Vec::new()),
            ],
        };
        let machines = MachineSet {
            default: default_machine.clone(),
            per_rhei: BTreeMap::from([("review".to_string(), review_machine)]),
        };

        let report = validate_with_machine_set(&rhei, &machines);
        assert!(
            !report.has_errors(),
            "states valid under their owning machines must pass; got:\n{}",
            report.errors.join("\n")
        );

        // The same graph under one machine fails: `draft`/`done` are not
        // states of the default machine.
        let single = validate_with_machine(&rhei, &default_machine);
        assert!(
            single.errors.iter().any(|e| e.contains("invalid state 'draft'")),
            "single-machine validation should reject review states; got:\n{}",
            single.errors.join("\n")
        );

        // Cross-machine prior-order coherence: review.1 is terminal ('done')
        // while its prior plain.1 regresses to pending → warning, judged under
        // the prior's own (default) machine.
        let mut regressed = rhei.clone();
        regressed.tasks[0].state = "pending".to_string();
        let report = validate_with_machine_set(&regressed, &machines);
        assert!(
            report.warnings.iter().any(|w| w.contains("Task review.1")
                && w.contains("prerequisites are unsatisfied")
                && w.contains("plain.1 (pending)")),
            "cross-machine prior coherence should warn; got:\n{}",
            report.warnings.join("\n")
        );
    }

    /// §FS-rhei-plan-language.3.1: a kind keyword on a **Prior:** reference
    /// must match the referenced node's declared kind.
    #[test]
    fn reports_prior_kind_mismatch() {
        let input = r#"# Rhei: Example

---
structure:
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: Design schema
**State:** pending

### Bug 2: Fix login
**State:** pending

### Task 3: Ship
**State:** pending
**Prior:** Task 2, Bug 1
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sample_machine());

        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 3 **Prior:** kind keyword 'Task' does not match Task 2")
                && joined.contains("declared 'Bug'"),
            "missing Task->Bug mismatch; got:\n{joined}"
        );
        assert!(
            joined.contains("Task 3 **Prior:** kind keyword 'Bug' does not match Task 1")
                && joined.contains("declared 'Task'"),
            "missing Bug->Task mismatch; got:\n{joined}"
        );
    }

    /// §FS-rhei-plan-language.3.1: an undeclared kind keyword is reported as
    /// such; matching keywords and bare ids stay silent.
    #[test]
    fn reports_undeclared_prior_kind_keyword() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Design schema
**State:** pending

### Task 2: Build API
**State:** pending
**Prior:** Banana 1

### Task 3: Ship
**State:** pending
**Prior:** Task 1, 2
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sample_machine());

        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 2 **Prior:** kind keyword 'Banana' does not match Task 1")
                && joined.contains("'Banana' is not a declared node kind"),
            "missing undeclared-kind error; got:\n{joined}"
        );
        assert!(
            !joined.contains("Task 3"),
            "matching keyword or bare id wrongly flagged; got:\n{joined}"
        );
    }

    /// §FS-rhei-plan-language.3.1: a pasted task title parses as
    /// `<kind> <id>`; the error names that reading instead of inventing a
    /// phantom task id out of the title's second word.
    #[test]
    fn hints_that_an_unresolvable_prior_may_be_a_title() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Design schema
**State:** pending

### Task 2: Build API
**State:** pending
**Prior:** Design schema
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sample_machine());

        let joined = report.errors.join("\n");
        assert!(
            joined.contains("'Design' is not a declared node kind")
                && joined.contains("If the reference is a task title"),
            "missing title hint; got:\n{joined}"
        );
        assert!(
            !joined.contains("depends on missing Task"),
            "generic missing-task error should be replaced by the title hint; got:\n{joined}"
        );
    }

    /// §FS-rhei-plan-language.3.1: duplicate **Prior:** references are errors.
    #[test]
    fn reports_duplicate_prior_references() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Design schema
**State:** pending

### Task 2: Build API
**State:** pending
**Prior:** 1, 1
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sample_machine());

        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 2 lists Task 1 more than once in **Prior:**"),
            "missing duplicate-prior error; got:\n{joined}"
        );
    }

    /// §FS-rhei-validate.4: a ticket that went terminal ahead of its prior
    /// leaves readiness and `--blocked`, so validation is the only surface
    /// that can still reveal the contradiction.
    #[test]
    fn warns_when_a_task_completed_ahead_of_its_prior() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: First
**State:** pending

### Task 2: Second
**State:** completed
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "must stay a warning: {:?}", report.errors);
        let joined = report.warnings.join("\n");
        assert!(
            joined.contains("Task 2 is 'completed' but its prerequisites are unsatisfied")
                && joined.contains("Task 1 (pending)"),
            "did not find expected warning; got:\n{}",
            joined
        );
    }

    #[test]
    fn does_not_warn_when_priors_are_satisfied_or_task_is_open() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: First
**State:** completed

### Task 2: Second
**State:** completed
**Prior:** Task 1

### Task 3: Third
**State:** pending
**Prior:** Task 2
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
        assert!(
            !report.warnings.iter().any(|w| w.contains("prerequisites are unsatisfied")),
            "unexpected prior-order warning: {:?}",
            report.warnings
        );
    }

    /// One consumer is enough to trigger the advisory, while a second consumer
    /// must not duplicate a graph-level warning.
    // §FS-rhei-validate.4 §FS-rhei-plan-language.3.12
    #[test]
    fn consumes_is_one_graph_level_advisory_for_any_number_of_consumers() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Producer
**State:** completed
**Provides:** evidence

### Task 2: Consumer one
**State:** pending
**Prior:** Task 1
**Consumes:** 1:evidence

### Task 3: Consumer two
**State:** pending
**Prior:** Task 1
**Consumes:** 1:evidence
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sample_machine());

        assert!(!report.has_errors(), "the advisory is non-fatal: {:?}", report.errors);
        assert_eq!(
            report.warnings,
            vec!["**Consumes:** declares export data-flow for prompt injection, not filesystem visibility. Workers can read undeclared sibling exports under runtime/exports/. For a blind round, schedule participants concurrently and brief them not to inspect sibling exports; neither measure enforces blindness once an export exists."]
        );
    }

    /// Plans without `Consumes` do not pay for advice that does not apply.
    // §FS-rhei-validate.4
    #[test]
    fn a_graph_without_consumes_gets_no_consumes_advisory() {
        let rhei = parse(
            r#"# Rhei: Example
## Tasks

### Task 1: Ordinary work
**State:** pending
"#,
        )
        .expect("parse ok");
        let report = validate_with_machine(&rhei, &sample_machine());

        assert!(
            report.warnings.iter().all(|warning| !warning.contains("**Consumes:**")),
            "unexpected advisory: {:?}",
            report.warnings
        );
    }

    /// Watch mode asks the validator for a fresh report on each pass. Each
    /// report owns one graph-level advisory, with no per-consumer duplication.
    // §FS-rhei-validate.5
    #[test]
    fn consumes_advisory_occurs_once_in_each_validation_pass() {
        let rhei = parse(
            r#"# Rhei: Example
## Tasks

### Task 1: Producer
**State:** completed
**Provides:** evidence

### Task 2: Consumer
**State:** pending
**Consumes:** 1:evidence
"#,
        )
        .expect("parse ok");

        for pass in 1..=2 {
            let report = validate_with_machine(&rhei, &sample_machine());
            assert_eq!(
                report.warnings.iter().filter(|warning| warning.starts_with("**Consumes:**"))
                    .count(),
                1,
                "validation pass {pass}: {:?}",
                report.warnings
            );
        }
    }

    #[test]
    fn rejects_child_prior_to_parent() {
        let input = r#"# Rhei: Example
## Tasks

### Task fetch-prs: Fetch pull requests
**State:** completed

#### Task fetch-prs.ci-failure-5227: Triage CI failure
**State:** pending
**Prior:** Task fetch-prs
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected parent-as-prior validation error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains(
                "Task fetch-prs.ci-failure-5227 cannot list ancestor Task fetch-prs as **Prior:**"
            ),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn rejects_descendant_prior_to_ancestor() {
        let input = r#"# Rhei: Example
---
structure:
  maxLevels: 3
---

## Tasks

### Task release: Release
**State:** pending

#### Task release.notes: Notes
**State:** pending

##### Task release.notes.diff: Diff notes
**State:** pending
**Prior:** Task release
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected ancestor-as-prior validation error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains(
                "Task release.notes.diff cannot list ancestor Task release as **Prior:**"
            ),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn ok_when_all_dependencies_exist_named_and_numeric() {
        let input = r#"# Rhei: Example
## Tasks

### Task init: Initialize
**State:** pending

### Task 2: B
**State:** in-progress
**Prior:** Task init

### Task 1: A
**State:** completed
**Prior:** Task 2, Task init
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn rejects_mutually_exclusive_task_execution_overrides() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending
**Model:** gpt-5
**Target:** codex:openai:gpt-5-codex
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine_with_models();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected mutual exclusion error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("declares both **Model:** and **Target:**"),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn rejects_task_model_not_declared_by_machine() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending
**Model:** missing-model
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine_with_models();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected model membership error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("declares **Model:** 'missing-model'"),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn rejects_task_override_on_fanout_state() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** review
**Model:** gpt-5
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = StateMachine::from_yaml_str(
            r#"
name: fanout
version: 1
models: [gpt-5, claude]
states:
  review:
    all_models: [gpt-5, claude]
    agent: codex
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#,
        )
        .expect("states load");
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected fanout override error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("state 'review' is a fanout state"),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn rejects_task_override_on_target_locked_state() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** locked
**Target:** codex:openai:gpt-5-codex
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = StateMachine::from_yaml_str(
            r#"
name: locked
version: 1
models: [gpt-5]
states:
  locked:
    target: codex:openai:gpt-5-codex
    target_locked: true
  completed:
    final: true
transitions:
  - from: locked
    to: completed
"#,
        )
        .expect("states load");
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected target_locked override error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("state 'locked' has target_locked: true"),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn missing_state_is_parse_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
"#;
        let err = parse(input).unwrap_err();
        assert!(
            err.message.contains("missing mandatory **State:**"),
            "expected parse error about missing state; got: {}",
            err.message
        );
    }

    #[test]
    fn reports_invalid_state_with_allowed_list() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** invalid_state
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected invalid state error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("invalid state"),
            "did not find 'invalid state' in errors:\n{}",
            joined
        );
        assert!(
            joined.contains("Allowed: ["),
            "did not include 'Allowed: [...]' list:\n{}",
            joined
        );
        for s in ["pending", "in-progress", "completed"] {
            assert!(joined.contains(s), "allowed list missing state '{}'; errors:\n{}", s, joined);
        }
    }

    #[test]
    fn accepts_valid_states_and_escaped_spaces() {
        // Custom states definition with a state containing a space
        let yaml = r#"
name: sm-escaped
version: 1
states:
  "in progress": { description: "with space" }
  done: { description: "done", final: true }
transitions:
  - from: "in progress"
    to: done
"#;
        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** `in progress`
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &machine);

        assert!(
            !report.has_errors(),
            "unexpected errors validating escaped-space state: {:?}",
            report.errors
        );
    }

    #[test]
    fn ok_when_all_tasks_have_valid_state() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
**State:** in-progress

### Task 3: C
**State:** completed
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn detects_two_node_cycle() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 2

### Task 2: B
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
        assert!(joined.contains("2"), "should mention task 2; got:\n{}", joined);
    }

    #[test]
    fn detects_three_node_cycle() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 2

### Task 2: B
**State:** in-progress
**Prior:** Task 3

### Task 3: C
**State:** completed
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        // At least two task ids should be mentioned; typically all three.
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
        assert!(joined.contains("2"), "should mention task 2; got:\n{}", joined);
    }

    #[test]
    fn detects_self_cycle() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected self-cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
    }

    #[test]
    fn passes_on_dag() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
**State:** in-progress
**Prior:** Task 1

### Task 3: C
**State:** completed
**Prior:** Task 2
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors in DAG case: {:?}", report.errors);
    }

    #[test]
    fn no_false_cycle_with_missing_dependency() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 9

### Task 2: B
**State:** in-progress
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected missing dependency error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 1 depends on missing Task 9"),
            "did not find expected missing-dep message; got:\n{}",
            joined
        );
        assert!(
            !joined.contains("Circular dependency detected"),
            "should not report a cycle when only a dependency is missing; got:\n{}",
            joined
        );
    }

    // ---- Child/parent id-extension semantics ----
    //
    // The "subtask numbering" validator has been removed; the rule that a
    // child id must extend its parent's id by exactly one segment is now
    // enforced by the parser (see `crates/rhei-core/src/parser.rs`), which
    // rejects malformed child headings with a parse error before validation
    // runs. The old `mismatched_parent_number_errors`,
    // `named_task_subtasks_produce_error`, `mixed_tasks_ok_and_error`, and
    // `multiple_subtasks_some_bad` tests were deleted accordingly — their
    // inputs no longer parse, so there's nothing left for the validator to
    // check.

    #[test]
    fn valid_subtask_numbering_ok() {
        let input = r#"# Rhei: Example
## Tasks

### Task 3: C
**State:** pending

#### Task 3.1: First
**State:** pending
#### Task 3.2: Second
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn terminal_parent_with_non_terminal_subtask_errors() {
        let input = r#"# Rhei: Example
## Tasks

### Task 2: Parent
**State:** completed

#### Task 2.1: Still open
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = StateMachine::from_yaml_str(
            r#"
name: terminal-parent-test
version: 1.0
states:
  pending: { description: "not started" }
  completed: { description: "done", final: true }
transitions:
  - from: pending
    to: completed
"#,
        )
        .expect("states load");
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected terminal parent coherence error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 2 is in terminal state 'completed'"),
            "expected terminal parent state in error; got:\n{}",
            joined
        );
        assert!(
            joined
                .contains("descendant Task 2.1 ('Still open') is in non-terminal state 'pending'"),
            "expected non-terminal descendant in error; got:\n{}",
            joined
        );
    }

    #[test]
    fn terminal_parent_with_terminal_subtasks_is_valid() {
        let input = r#"# Rhei: Example
## Tasks

### Task 2: Parent
**State:** completed

#### Task 2.1: Done
**State:** completed
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = StateMachine::from_yaml_str(
            r#"
name: terminal-parent-test
version: 1.0
states:
  pending: { description: "not started" }
  completed: { description: "done", final: true }
transitions:
  - from: pending
    to: completed
"#,
        )
        .expect("states load");
        let report = validate_with_machine(&rhei, &sm);

        assert!(
            !report.has_errors(),
            "terminal parent with terminal subtasks should validate: {:?}",
            report.errors
        );
    }

    #[test]
    fn duplicate_sibling_child_id_is_rejected() {
        // The new validator checks that sibling ids under a common parent are
        // unique, replacing the old ad-hoc "subtask uniqueness" rule.
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

#### Task 1.1: First
**State:** pending

#### Task 1.1: Duplicate
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "duplicate sibling id should be rejected");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Duplicate sibling task id: Task 1.1")
                && joined.contains("under Task 1"),
            "expected duplicate-sibling message; got:\n{}",
            joined
        );
    }

    #[test]
    fn prior_without_state_is_parse_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**Prior:** Task 2

### Task 2: B
**State:** pending
"#;
        let err = parse(input).unwrap_err();
        assert!(
            err.message.contains("**State:** must appear before **Prior:**"),
            "expected parse error about ordering; got: {}",
            err.message
        );
    }
