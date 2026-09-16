    #[test]
    fn validation_report_extend_merges_errors_and_warnings() {
        let mut base =
            ValidationReport {
                errors: vec!["e1".to_string()],
                warnings: vec!["w1".to_string()],
                help: Vec::new(),
            };
        let other =
            ValidationReport {
                errors: vec!["e2".to_string()],
                warnings: vec!["w2".to_string()],
                help: Vec::new(),
            };

        base.extend(other);

        assert_eq!(base.errors, vec!["e1".to_string(), "e2".to_string()]);
        assert_eq!(base.warnings, vec!["w1".to_string(), "w2".to_string()]);
    }

    #[test]
    fn unit_type_validate_returns_ok_report() {
        let report = ().validate();

        assert_eq!(report, ValidationReport::ok());
        assert!(!report.has_errors());
    }

    // ---- Markdown link validation tests ----

    #[test]
    fn extract_markdown_links_finds_all_links() {
        let text = "See [docs](docs/spec.md) and [site](https://example.com) for details.";
        let links = extract_markdown_links(text);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], ("docs".to_string(), "docs/spec.md".to_string()));
        assert_eq!(links[1], ("site".to_string(), "https://example.com".to_string()));
    }

    #[test]
    fn extract_markdown_links_handles_no_links() {
        let links = extract_markdown_links("No links here.");
        assert!(links.is_empty());
    }

    #[test]
    fn is_non_file_link_classifies_correctly() {
        assert!(is_non_file_link("https://example.com"));
        assert!(is_non_file_link("http://example.com"));
        assert!(is_non_file_link("mailto:user@example.com"));
        assert!(is_non_file_link("#section"));
        assert!(!is_non_file_link("docs/spec.md"));
        assert!(!is_non_file_link("../README.md"));
    }

    #[test]
    fn link_validation_reports_missing_file() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = r#"# Rhei: Example
## Overview
See [the spec](specs/nonexistent.md) for details.

## Tasks

### Task 1: A
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(report.has_errors(), "expected missing link error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("nonexistent.md") && joined.contains("does not exist"),
            "expected broken link error; got:\n{}",
            joined
        );
    }

    #[test]
    fn link_validation_passes_when_file_exists() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let specs_dir = dir.path().join("specs");
        fs::create_dir_all(&specs_dir).expect("mkdir");
        fs::write(specs_dir.join("real.md"), "# Real spec").expect("write");

        let input = r#"# Rhei: Example
## Overview
See [the spec](specs/real.md) for details.

## Tasks

### Task 1: A
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn link_validation_ignores_external_urls() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

See [docs](https://example.com/docs) and [anchor](#overview) for info.
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(!report.has_errors(), "external links should not be checked: {:?}", report.errors);
    }

    #[test]
    fn link_validation_strips_fragment_from_file_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::write(dir.path().join("guide.md"), "# Guide").expect("write");

        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

See [section](guide.md#usage) for details.
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(
            !report.has_errors(),
            "file exists, fragment should be stripped: {:?}",
            report.errors
        );
    }

    #[test]
    fn link_validation_checks_task_and_subtask_content() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

See [missing](nowhere.md) for context.

#### Task 1.1: Sub
**State:** pending
Also see [gone](also-gone.md).
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(report.has_errors());
        let joined = report.errors.join("\n");
        assert!(joined.contains("nowhere.md"), "should report task link; got:\n{}", joined);
        assert!(joined.contains("also-gone.md"), "should report subtask link; got:\n{}", joined);
    }

    #[test]
    fn link_validation_skipped_without_base_path() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

See [missing](nowhere.md) for context.
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        // validate() does not pass a base path, so link checking is skipped
        let report = validate_with_machine(&rhei, &sm);

        assert!(
            !report.has_errors(),
            "without base path, links should not be checked: {:?}",
            report.errors
        );
    }

    #[test]
    fn result_block_on_non_terminal_task_is_invalid_even_when_file_exists() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let results_dir = dir.path().join("runtime/results");
        fs::create_dir_all(&results_dir).expect("mkdir results");
        fs::write(results_dir.join("1.md"), "## pending → completed\n").expect("write result");

        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

> **Result:** [1](runtime/results/1.md)
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(report.has_errors(), "expected result block lifecycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("non-terminal state") && joined.contains("result block"),
            "expected non-terminal result block error; got:\n{}",
            joined
        );
    }

    #[test]
    fn result_block_must_match_enclosing_task() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** completed

> **Result:** [2](runtime/results/2.md)
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate(&rhei);

        assert!(report.has_errors(), "expected result block identity errors");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("must link '[1](runtime/results/1.md)'"),
            "got:\n{}",
            joined
        );
        assert!(joined.contains("got '[2](runtime/results/2.md)'"), "got:\n{}", joined);
    }

    #[test]
    fn rejects_program_on_gating_state() {
        let yaml = r#"name: demo
version: 1
states:
  review:
    description: Human review
    gating: true
    program: "echo nope"
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject program on gating");
        assert!(err.to_string().contains("cannot declare a 'program'"));
    }

    #[test]
    fn rejects_exit_code_transition_from_non_program_state() {
        let yaml = r#"name: demo
version: 1
states:
  pending:
    description: Agent work
    agent: codex
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
    exit_code: 0
"#;

        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject exit_code on non-program");
        assert!(err.to_string().contains("declares 'exit_code'"));
    }

    // ---- MCP servers / skills per-state validation ----

    #[test]
    fn state_mcp_servers_accepts_string_and_object_forms() {
        let yaml = r#"
name: mcp-basic
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - postgres
      - id: grafana
        optional: true
    skills:
      - test-authoring
      - id: adhoc
        path: ./skills/adhoc
        optional: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
"#;
        let sm = StateMachine::from_yaml_str(yaml).expect("should accept both forms");
        let pending = sm.states.get("pending").expect("pending state");
        let mcp = pending.mcp_servers.as_ref().expect("mcp_servers declared");
        assert_eq!(mcp.len(), 2);
        assert_eq!(mcp[0].id(), "postgres");
        assert!(!mcp[0].is_optional());
        assert_eq!(mcp[1].id(), "grafana");
        assert!(mcp[1].is_optional());

        let skills = pending.skills.as_ref().expect("skills declared");
        assert_eq!(skills.len(), 2);
        assert!(
            matches!(&skills[1], StateSkillEntry::Object(obj) if obj.path.as_deref() == Some("./skills/adhoc"))
        );
    }

    #[test]
    fn state_mcp_servers_empty_list_preserved_as_clear_marker() {
        let yaml = r#"
name: mcp-clear
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers: []
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
"#;
        let sm = StateMachine::from_yaml_str(yaml).expect("empty list is valid");
        let pending = sm.states.get("pending").expect("pending");
        assert_eq!(pending.mcp_servers.as_deref().map(<[_]>::len), Some(0));
    }

    #[test]
    fn state_mcp_servers_rejects_duplicate_ids() {
        let yaml = r#"
name: mcp-dup
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - postgres
      - id: postgres
        optional: true
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("duplicate ids");
        assert!(err.to_string().contains("duplicate mcp_servers id 'postgres'"));
    }

    #[test]
    fn state_mcp_servers_rejects_both_command_and_url() {
        let yaml = r#"
name: mcp-inline-both
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - id: inline
        command: ["mcp-server"]
        url: "https://example/mcp"
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("mutually exclusive");
        assert!(err.to_string().contains("both 'command' and 'url'"));
    }

    #[test]
    fn state_mcp_servers_rejected_on_gating_state() {
        let yaml = r#"
name: mcp-gating
version: 1.0
states:
  pending:
    description: Work
    gating: true
    mcp_servers: [postgres]
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("gating excludes mcp");
        assert!(err.to_string().contains("gating"));
    }

    #[test]
    fn state_mcp_servers_rejected_on_program_state() {
        let yaml = r#"
name: mcp-program
version: 1.0
states:
  build:
    description: Build
    program: "make"
    mcp_servers: [postgres]
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("program excludes mcp");
        assert!(err.to_string().contains("program"));
    }

    #[test]
    fn state_skills_rejected_on_terminal_state() {
        let yaml = r#"
name: skill-final
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
  completed:
    description: Done
    final: true
    skills: [review-checklist]
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("final excludes skills");
        assert!(err.to_string().contains("final"));
    }

    #[test]
    fn template_condition_accepts_mcp_and_skill_when_declared() {
        let yaml = r#"
name: cond-ok
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    instructions: |
      {if mcp.postgres.available}Use Postgres.{endif}
      {if skill.test-authoring.available}Use test skill.{endif}
    mcp_servers: [postgres]
    skills: [test-authoring]
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
"#;
        StateMachine::from_yaml_str(yaml).expect("valid references");
    }

    #[test]
    fn template_condition_rejects_mcp_not_declared() {
        let yaml = r#"
name: cond-bad-mcp
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    instructions: "{if mcp.other.available}X{endif}"
    mcp_servers: [postgres]
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("other is not declared");
        assert!(err.to_string().contains("'other'"));
        assert!(err.to_string().contains("mcp_servers"));
    }

    #[test]
    fn transition_mcp_unavailable_accepts_true_and_list() {
        let yaml = r#"
name: trig-ok
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers: [postgres]
  tooling-missing:
    description: Blocked
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: tooling-missing
    mcp_unavailable: true
  - from: pending
    to: tooling-missing
    mcp_unavailable: [postgres]
  - from: tooling-missing
    to: completed
"#;
        StateMachine::from_yaml_str(yaml).expect("valid trigger shapes");
    }

    #[test]
    fn transition_mcp_unavailable_rejects_false() {
        let yaml = r#"
name: trig-false
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
  tooling-missing:
    description: Blocked
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: tooling-missing
    mcp_unavailable: false
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("false is invalid");
        assert!(err.to_string().contains("mcp_unavailable: false"));
    }

    #[test]
    fn transition_mcp_unavailable_rejects_on_program_state() {
        let yaml = r#"
name: trig-prog
version: 1.0
states:
  build:
    description: Build
    program: "make"
  failed:
    description: Build failed
    final: true
transitions:
  - from: build
    to: failed
    mcp_unavailable: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("program source state");
        assert!(err.to_string().contains("agent-only"));
    }

    // ---- profiles / node_policy ----

    // §FS-rhei-panta.6.3: a plan authored before ticket ids gained their rhei
    // prefix keeps validating; the qualified form stays canonical.
    #[test]
    fn result_block_accepts_legacy_rhei_local_link_and_qualified_link() {
        fn qualified_plan(link: &str) -> crate::ast::Rhei {
            let input = format!(
                "# Rhei: Legacy\n\n## Tasks\n\n### Task 1: Old work\n**State:** completed\n\n> **Result:** {link}\n"
            );
            let rhei = parse(&input).expect("parse ok");
            let workspace = crate::workspace::implicit_panta_from_file_rhei(
                rhei,
                std::path::Path::new("legacy.rhei.md"),
            )
            .expect("wrap as implicit panta");
            workspace.rhei
        }

        let machine = sample_machine();

        // Legacy rhei-local link is accepted.
        let report = validate_with_machine(
            &qualified_plan("[1](runtime/results/1.md)"),
            &machine,
        );
        let link_errors: Vec<_> =
            report.errors.iter().filter(|err| err.contains("result block")).collect();
        assert!(link_errors.is_empty(), "legacy link should validate: {link_errors:?}");

        // Canonical qualified link is accepted.
        let report = validate_with_machine(
            &qualified_plan("[legacy.1](runtime/results/legacy.1.md)"),
            &machine,
        );
        let link_errors: Vec<_> =
            report.errors.iter().filter(|err| err.contains("result block")).collect();
        assert!(link_errors.is_empty(), "qualified link should validate: {link_errors:?}");

        // An unrelated id is still rejected.
        let report = validate_with_machine(
            &qualified_plan("[other.9](runtime/results/other.9.md)"),
            &machine,
        );
        assert!(
            report.errors.iter().any(|err| err.contains("result block must link")),
            "unrelated link must still error: {:?}",
            report.errors
        );

        // §FS-rhei-panta.6.3: text and target are validated as a pair, so a
        // link mixing the qualified and legacy forms is an error either way.
        for mixed in
            ["[1](runtime/results/legacy.1.md)", "[legacy.1](runtime/results/1.md)"]
        {
            let report = validate_with_machine(&qualified_plan(mixed), &machine);
            assert!(
                report.errors.iter().any(|err| err.contains("result block must link")),
                "mixed-form link {mixed} must error: {:?}",
                report.errors
            );
        }
    }
