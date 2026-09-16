// §FS-rhei-states.4.4: Reusable state prompt templates.

    /// Build a machine whose `review` state carries `state_body`.
    fn prompt_machine(state_body: &str) -> String {
        format!(
            r#"
name: prompt-template-test
version: 1.0
states:
  review:
    description: Review
{state_body}
  done:
    description: Done
    final: true
transitions:
  - from: review
    to: done
"#
        )
    }

    /// Write `states.yaml` plus a `prompt_templates/` directory, then load it.
    fn load_with_templates(
        state_body: &str,
        templates: &[(&str, &str)],
    ) -> Result<StateMachine, StateMachineLoadError> {
        let dir = tempfile::tempdir().expect("tempdir");
        let states_path = dir.path().join("states.yaml");
        std::fs::write(&states_path, prompt_machine(state_body)).expect("write states.yaml");
        if !templates.is_empty() {
            let templates_dir = dir.path().join("prompt_templates");
            std::fs::create_dir_all(&templates_dir).expect("create prompt_templates");
            for (name, body) in templates {
                std::fs::write(templates_dir.join(name), body).expect("write prompt template");
            }
        }
        StateMachine::from_yaml_file(&states_path)
    }

    const REVIEW_TEMPLATE: &str = "You are a {review_role}.\n\nReview Task {task_id}.\n";

    fn review_state_body() -> &'static str {
        r#"    prompt_template:
      name: artifact-review
      values:
        review_role: API reviewer
        task_id: "{task_id}"
"#
    }

    #[test]
    fn loads_markdown_files_as_prompt_templates() {
        let machine = load_with_templates(
            review_state_body(),
            &[("artifact-review.md", REVIEW_TEMPLATE), ("other.md", "Other prompt text.\n")],
        )
        .expect("machine should load");

        assert_eq!(
            machine.prompt_templates.keys().collect::<Vec<_>>(),
            vec!["artifact-review", "other"],
            "file stems become template ids, in sorted order"
        );
        assert_eq!(machine.prompt_templates["other"].instructions, "Other prompt text.\n");
    }

    #[test]
    fn ignores_non_markdown_files_and_subdirectories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let states_path = dir.path().join("states.yaml");
        std::fs::write(&states_path, prompt_machine(""))
            .expect("write states.yaml");
        let templates_dir = dir.path().join("prompt_templates");
        std::fs::create_dir_all(templates_dir.join("nested")).expect("create nested");
        std::fs::write(templates_dir.join("keep.md"), "Kept.\n").expect("write keep");
        std::fs::write(templates_dir.join("notes.txt"), "Ignored.\n").expect("write notes");
        std::fs::write(templates_dir.join("nested/deep.md"), "Ignored.\n").expect("write deep");

        let machine = StateMachine::from_yaml_file(&states_path).expect("machine should load");

        assert_eq!(machine.prompt_templates.keys().collect::<Vec<_>>(), vec!["keep"]);
    }

    #[test]
    fn missing_prompt_templates_directory_is_not_an_error() {
        let machine =
            load_with_templates("", &[]).expect("machine should load");

        assert!(machine.prompt_templates.is_empty());
    }

    #[test]
    fn composes_template_text_before_inline_instructions() {
        let body = format!("{}    instructions: Then do the inline part.\n", review_state_body());
        let machine = load_with_templates(&body, &[("artifact-review.md", REVIEW_TEMPLATE)])
            .expect("machine should load");

        let effective = machine
            .effective_instructions(&machine.states["review"])
            .expect("state has instructions");

        assert_eq!(
            effective,
            "You are a API reviewer.\n\nReview Task {task_id}.\n\nThen do the inline part.",
            "template text is substituted and emitted before the inline text; \
             a runtime variable passed through values survives for the runtime pass"
        );
    }

    #[test]
    fn state_without_template_keeps_inline_instructions_only() {
        let machine =
            load_with_templates("    instructions: Just inline.\n", &[]).expect("machine loads");

        assert_eq!(
            machine.effective_instructions(&machine.states["review"]).as_deref(),
            Some("Just inline.")
        );
        assert!(machine.effective_instructions(&machine.states["done"]).is_none());
    }

    #[test]
    fn personality_stays_inline_only() {
        let machine = load_with_templates(
            &format!("{}    personality: Inline framing.\n", review_state_body()),
            &[("artifact-review.md", REVIEW_TEMPLATE)],
        )
        .expect("machine loads");

        assert_eq!(
            machine.effective_personality(&machine.states["review"]).as_deref(),
            Some("Inline framing."),
            "prompt templates contribute instructions only"
        );
        assert!(machine.effective_personality(&machine.states["done"]).is_none());
    }

    #[test]
    fn string_form_reference_selects_a_template() {
        let machine =
            load_with_templates("    prompt_template: artifact-review\n", &[("artifact-review.md", "Fixed prompt text.\n")])
                .expect("machine loads");

        assert_eq!(
            machine.effective_instructions(&machine.states["review"]).as_deref(),
            Some("Fixed prompt text.")
        );
    }

    #[test]
    fn rejects_placeholder_without_a_supplied_value() {
        let err = load_with_templates(
            r#"    prompt_template:
      name: artifact-review
      values:
        review_role: API reviewer
"#,
            &[("artifact-review.md", REVIEW_TEMPLATE)],
        )
        .expect_err("task_id is not supplied");

        let message = err.to_string();
        assert!(message.contains("{task_id}"), "{message}");
        assert!(message.contains("does not supply 'task_id'"), "{message}");
    }

    #[test]
    fn treats_non_identifier_braces_as_literal() {
        let machine = load_with_templates(
            "    prompt_template: artifact-review\n",
            &[("artifact-review.md", "Emit JSON like {\"id\": 1} and {output.report.path}.\n")],
        )
        .expect("JSON-ish and dotted braces need no values");

        assert_eq!(
            machine.effective_instructions(&machine.states["review"]).as_deref(),
            Some("Emit JSON like {\"id\": 1} and {output.report.path}."),
            "non-identifier brace content is left for later passes"
        );
    }

    #[test]
    fn escaped_braces_need_no_value_and_stay_escaped() {
        let machine = load_with_templates(
            "    prompt_template: artifact-review\n",
            &[("artifact-review.md", "Write \\{task_id\\} literally.\n")],
        )
        .expect("escaped braces are not placeholders");

        assert_eq!(
            machine.effective_instructions(&machine.states["review"]).as_deref(),
            Some("Write \\{task_id\\} literally."),
            "unescaping is the runtime pass's job, so the escapes survive this stage"
        );
    }

    const CONDITIONAL_TEMPLATE: &str =
        "{if input.spec.exists}Read the spec.{else}No spec.{endif}\n";

    #[test]
    fn conditional_control_tokens_need_no_value() {
        load_with_templates(
            r#"    prompt_template: artifact-review
    inputs:
      - name: spec
        path: spec.md
"#,
            &[("artifact-review.md", CONDITIONAL_TEMPLATE)],
        )
        .expect("`if`, `else`, and `endif` are control tokens, not placeholders needing values");
    }

    #[test]
    fn template_text_is_checked_against_the_selecting_state_inputs() {
        let err = load_with_templates(
            "    prompt_template: artifact-review\n",
            &[("artifact-review.md", CONDITIONAL_TEMPLATE)],
        )
        .expect_err("the state selecting the template declares no 'spec' input");

        assert!(
            err.to_string().contains("'spec' is not a declared input"),
            "conditions inside reusable text are checked against the state that selects it: {err}"
        );
    }

    #[test]
    fn rejects_unknown_template_reference_naming_the_declared_ones() {
        let err = load_with_templates(
            "    prompt_template: typo-review\n",
            &[("artifact-review.md", "Review it.\n")],
        )
        .expect_err("typo-review does not exist");

        let message = err.to_string();
        assert!(message.contains("unknown prompt template 'typo-review'"), "{message}");
        assert!(message.contains("artifact-review"), "the error lists what is declared: {message}");
    }

    #[test]
    fn explains_that_string_loading_cannot_resolve_prompt_templates() {
        let err = StateMachine::from_yaml_str(&prompt_machine(
            "    prompt_template: artifact-review\n",
        ))
        .expect_err("a string has no sibling directory to read");

        let message = err.to_string();
        assert!(
            message.contains("no prompt templates were loaded"),
            "the error explains the cause rather than blaming the id: {message}"
        );
        assert!(message.contains("prompt_templates/artifact-review.md"), "{message}");
    }

    #[test]
    fn rejects_empty_prompt_template_file_naming_the_path() {
        let err = load_with_templates("", &[("artifact-review.md", "  \n")])
            .expect_err("an empty prompt file is an authoring mistake");

        assert!(
            err.to_string().contains("prompt_templates/artifact-review.md"),
            "{}",
            err.to_string()
        );
    }

    #[test]
    fn rejects_non_scalar_and_non_identifier_values() {
        let err = load_with_templates(
            r#"    prompt_template:
      name: artifact-review
      values:
        review_role: [a, b]
"#,
            &[("artifact-review.md", "You are a {review_role}.\n")],
        )
        .expect_err("a list is not a scalar");
        assert!(err.to_string().contains("must be a scalar value"), "{}", err.to_string());

        let err = load_with_templates(
            r#"    prompt_template:
      name: artifact-review
      values:
        "not an identifier": x
"#,
            &[("artifact-review.md", "Review it.\n")],
        )
        .expect_err("keys must be identifiers");
        assert!(err.to_string().contains("expected an identifier"), "{}", err.to_string());
    }

    #[test]
    fn rejects_empty_reference_name() {
        let err = load_with_templates(
            "    prompt_template:\n      name: \"  \"\n",
            &[("artifact-review.md", "Review it.\n")],
        )
        .expect_err("an empty name selects nothing");

        assert!(
            err.to_string().contains("empty 'prompt_template' name"),
            "{}",
            err.to_string()
        );
    }

    #[test]
    fn rejects_inline_top_level_prompt_templates_block() {
        let yaml = r#"
name: inline-templates
version: 1.0
prompt_templates:
  artifact-review:
    instructions: Review it.
states:
  done:
    description: Done
    final: true
transitions: []
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("inline block is rejected");

        assert!(err.to_string().contains("sibling 'prompt_templates/' directory"), "{}", err);
    }

    #[test]
    fn rejects_legacy_prompt_templates_yaml_sibling() {
        let dir = tempfile::tempdir().expect("tempdir");
        let states_path = dir.path().join("states.yaml");
        std::fs::write(&states_path, prompt_machine(""))
            .expect("write states.yaml");
        std::fs::write(dir.path().join("prompt-templates.yaml"), "artifact-review: {}\n")
            .expect("write legacy file");

        let err = StateMachine::from_yaml_file(&states_path).expect_err("legacy file is rejected");

        assert!(err.to_string().contains("no longer supported"), "{}", err);
    }

    #[test]
    fn rejects_prompt_templates_path_that_is_a_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let states_path = dir.path().join("states.yaml");
        std::fs::write(&states_path, prompt_machine(""))
            .expect("write states.yaml");
        std::fs::write(dir.path().join("prompt_templates"), "not a directory\n")
            .expect("write file");

        let err = StateMachine::from_yaml_file(&states_path).expect_err("must be a directory");

        assert!(err.to_string().contains("must be a directory"), "{}", err);
    }

    #[test]
    fn rejects_nested_conditionals_inside_template_text() {
        let err = load_with_templates(
            "    prompt_template: artifact-review\n",
            &[(
                "artifact-review.md",
                "{if input.a.exists}{if input.b.exists}x{endif}{endif}\n",
            )],
        )
        .expect_err("nested conditionals are rejected in template text too");

        assert!(err.to_string().contains("nested"), "{}", err.to_string());
    }

    #[test]
    fn prompt_templates_dir_is_a_sibling_of_the_state_machine() {
        assert_eq!(
            prompt_templates_dir(Path::new("/plans/rhei/states.yaml")),
            PathBuf::from("/plans/rhei/prompt_templates")
        );
        assert_eq!(
            prompt_templates_dir(Path::new("states.yaml")),
            PathBuf::from("prompt_templates"),
            "a bare filename resolves against the current directory"
        );
    }
