    use super::*;
    use crate::parse;
    use std::fs;

    fn sample_machine() -> StateMachine {
        let yaml = r#"
name: test-sm
version: 1.0
states:
  pending: { description: "not started" }
  in-progress: { description: "doing" }
  completed: { description: "done", final: true }
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;
        StateMachine::from_yaml_str(yaml).expect("states load")
    }

    #[test]
    fn loads_state_machine_with_models_and_state_selectors() {
        let yaml = r#"
name: multi-model
version: 1.0
models:
  - gpt-5
  - claude-sonnet
states:
  draft:
    description: planned
    visits: 2
    all_models:
      - gpt-5
      - claude-sonnet
  review:
    description: reviewed
    model: claude-sonnet
  done:
    description: done
    final: true
transitions:
  - from: review
    to: done
  - from: draft
    to: done
"#;

        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        assert_eq!(machine.models, vec!["gpt-5", "claude-sonnet"]);
        assert_eq!(machine.states["draft"].visits, Some(2));
        assert_eq!(machine.states["draft"].all_models, vec!["gpt-5", "claude-sonnet"]);
        assert_eq!(machine.states["review"].model.as_deref(), Some("claude-sonnet"));
    }

    #[test]
    fn rejects_state_machine_with_unknown_state_model() {
        let yaml = r#"
name: multi-model
version: 1.0
models:
  - gpt-5
states:
  draft:
    description: planned
    model: claude-sonnet
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject unknown model");
        assert!(err.to_string().contains("references unknown model 'claude-sonnet'"));
    }

    #[test]
    fn rejects_state_machine_with_conflicting_state_model_selectors() {
        let yaml = r#"
name: multi-model
version: 1.0
models:
  - gpt-5
states:
  draft:
    description: planned
    all_models:
      - gpt-5
    model: gpt-5
"#;

        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject conflicting selectors");
        assert!(err.to_string().contains("cannot set both 'all_models' and 'model'"));
    }

    #[test]
    fn rejects_state_machine_with_unknown_all_models_entry() {
        let yaml = r#"
name: multi-model
version: 1.0
models:
  - gpt-5
states:
  draft:
    description: planned
    all_models:
      - claude-sonnet
"#;

        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject unknown all_models entry");
        assert!(err
            .to_string()
            .contains("references unknown model 'claude-sonnet' in 'all_models'"));
    }

    #[test]
    fn parses_execution_target_with_mode_and_provider() {
        let target = parse_execution_target("claude-code[yolo]:anthropic:claude-opus-4-7")
            .expect("target should parse");

        assert_eq!(target.agent, "claude-code");
        assert_eq!(target.mode.as_deref(), Some("yolo"));
        assert_eq!(target.provider.as_deref(), Some("anthropic"));
        assert_eq!(target.model, "claude-opus-4-7");
        assert_eq!(target.slug(), "claude-code-yolo-anthropic-claude-opus-4-7");
    }

    #[test]
    fn loads_state_machine_with_target_selectors() {
        let yaml = r#"
name: multi-target
version: 1.0
states:
  analyze:
    description: analyze
    all_targets:
      - claude-code[yolo]:anthropic:claude-opus-4-7
      - codex[yolo]:openai:gpt-5-codex
  done:
    description: done
    final: true
transitions:
  - from: analyze
    to: done
"#;

        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        assert_eq!(machine.states["analyze"].all_targets.len(), 2);
        assert_eq!(
            machine.states["analyze"].all_targets[0],
            "claude-code[yolo]:anthropic:claude-opus-4-7"
        );
    }

    #[test]
    fn rejects_explicit_empty_all_targets() {
        let yaml = r#"
name: multi-target
version: 1.0
states:
  analyze:
    description: analyze
    all_targets: []
  done:
    description: done
    final: true
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("empty all_targets rejected");
        assert!(err.to_string().contains("all_targets must be a non-empty list"));
    }

    #[test]
    fn rejects_state_machine_with_conflicting_target_and_model_selectors() {
        let yaml = r#"
name: multi-target
version: 1.0
models:
  - gpt-5
states:
  analyze:
    description: analyze
    target: codex[yolo]:openai:gpt-5-codex
    model: gpt-5
"#;

        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject conflicting selectors");
        assert!(err.to_string().contains("cannot combine 'target' or 'all_targets'"));
    }

    #[test]
    fn rejects_state_machine_with_zero_visits() {
        let yaml = r#"
name: multi-model
version: 1.0
states:
  draft:
    description: planned
    visits: 0
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject zero visits");
        assert!(err.to_string().contains("visits: 0"));
    }

    #[test]
    fn loads_state_machine_with_artifact_contracts() {
        let yaml = r#"
name: artifacts
version: 1.0
states:
  review:
    description: review work
    inputs:
      - name: implementation
        path: runtime/results/{task_id}.md
        format: markdown
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
        description: Review findings
  done:
    description: done
    final: true
transitions:
  - from: review
    to: done
"#;

        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        assert_eq!(machine.states["review"].inputs.len(), 1);
        assert_eq!(machine.states["review"].outputs.len(), 1);
        assert_eq!(machine.states["review"].outputs[0].name, "findings");
    }

    #[test]
    fn rejects_duplicate_artifact_names_in_same_state_field() {
        let yaml = r#"
name: artifacts
version: 1.0
states:
  review:
    description: review work
    outputs:
      - name: findings
        path: runtime/findings/a.md
      - name: findings
        path: runtime/findings/b.md
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject duplicate names");
        assert!(err.to_string().contains("duplicate artifact name 'findings'"));
    }

    #[test]
    fn rejects_absolute_artifact_paths() {
        let yaml = r#"
name: artifacts
version: 1.0
states:
  review:
    description: review work
    outputs:
      - name: findings
        path: /tmp/findings.md
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject absolute path");
        assert!(err.to_string().contains("must use a relative path"));
    }

    #[test]
    fn rejects_if_condition_referencing_undeclared_input() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    instructions: |
      {if input.typo.exists}
      Read the notes.
      {endif}
    inputs:
      - name: notes
        path: runtime/notes/{task_id}.md
        optional: true
"#;
        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject unknown input reference");
        assert!(err.to_string().contains("'typo' is not a declared input"));
    }

    #[test]
    fn rejects_if_condition_with_unsupported_form() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    instructions: |
      {if meta.flag}
      Extra instructions.
      {endif}
"#;
        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject unsupported condition");
        assert!(err.to_string().contains("not a recognised condition"));
    }

    #[test]
    fn accepts_if_condition_referencing_declared_input() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    instructions: |
      {if input.notes.exists}
      Read the notes.
      {endif}
    inputs:
      - name: notes
        path: runtime/notes/{task_id}.md
        optional: true
  done:
    description: done
    final: true
transitions:
  - from: implement
    to: done
"#;
        StateMachine::from_yaml_str(yaml).expect("valid condition should load");
    }

    #[test]
    fn rejects_nested_template_conditionals() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    instructions: |
      {if input.notes.exists}
      Read the notes.
      {if mcp.grafana.available}
      Check Grafana.
      {endif}
      {endif}
    inputs:
      - name: notes
        path: runtime/notes/{task_id}.md
        optional: true
    mcp_servers:
      - grafana
  done:
    description: done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("nested conditionals rejected");
        assert!(err.to_string().contains("nested conditional blocks"));
    }

    #[test]
    fn accepts_if_condition_in_personality_referencing_declared_input() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    personality: |
      {if input.context.exists}
      Use context from {input.context.path}.
      {endif}
    inputs:
      - name: context
        path: runtime/context/{task_id}.md
        optional: true
  done:
    description: done
    final: true
transitions:
  - from: implement
    to: done
"#;
        StateMachine::from_yaml_str(yaml).expect("valid condition in personality should load");
    }

    #[test]
    fn rejects_artifact_paths_that_escape_workspace_root() {
        let yaml = r#"
name: artifacts
version: 1.0
states:
  review:
    description: review work
    outputs:
      - name: findings
        path: ../../outside.md
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject escaping path");
        assert!(err.to_string().contains("escapes the workspace root"));
    }

    #[test]
    fn detects_missing_state_and_bad_dependency_and_cycle() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 3

#### Task 1.1: s
**State:** pending

### Task 2: B
**State:** invalid_state

### Task 3: C
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors());
        // Expect: Task 1 depends on 3 (exists), Task 3 depends on 1 => cycle
        // Also: Task 2 has invalid state
        let joined = report.errors.join("\n");
        assert!(joined.contains("invalid state"));
        assert!(joined.contains("Circular dependency detected"));
    }

    #[test]
    fn ok_when_valid() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

#### Task 1.1: s
**State:** pending

### Task 2: B
**State:** in-progress
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn warns_when_gating_state_declares_agent() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Review
**State:** review
"#;
        let yaml = r#"
name: gate-agent
version: 1.0
states:
  review:
    description: review
    gating: true
    agent: codex
  done:
    description: done
    final: true
transitions:
  - from: review
    to: done
"#;
        let rhei = parse(input).expect("parse ok");
        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        let report = validate_with_machine(&rhei, &machine);
        assert!(report.errors.is_empty(), "warning should not be fatal: {report:?}");
        assert!(
            report.warnings.iter().any(|warning| warning.contains("gating state")),
            "expected gating-agent warning, got {report:?}"
        );
    }

    #[test]
    fn accepts_counted_state_suffix_within_budget() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending-2
"#;
        let rhei = parse(input).expect("parse ok");
        let machine = StateMachine::from_yaml_str(
            r#"
name: example
version: 1.0
states:
  pending:
    description: queued
    visits: 3
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
        )
        .expect("states load");

        let report = validate_with_machine(&rhei, &machine);
        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn rejects_counted_state_suffix_of_one() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending-1
"#;
        let rhei = parse(input).expect("parse ok");
        let machine = StateMachine::from_yaml_str(
            r#"
name: example
version: 1.0
states:
  pending:
    description: queued
    visits: 3
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
        )
        .expect("states load");

        let report = validate_with_machine(&rhei, &machine);
        assert!(report.has_errors(), "expected counted suffix validation error");
        assert!(
            report.errors.iter().any(|err| err.contains("Visit suffix '-1' is not allowed")),
            "unexpected errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn rejects_counted_state_suffix_when_state_has_no_visits() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending-2
"#;
        let rhei = parse(input).expect("parse ok");
        let machine = sample_machine();

        let report = validate_with_machine(&rhei, &machine);
        assert!(report.has_errors(), "expected counted suffix validation error");
        assert!(
            report.errors.iter().any(|err| err.contains("does not declare 'visits'")),
            "unexpected errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn reports_missing_numeric_dependency() {
        let input = r#"# Rhei: Example
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
            "did not find expected message; got:\n{}",
            joined
        );
    }
