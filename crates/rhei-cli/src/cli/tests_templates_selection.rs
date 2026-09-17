// §FS-rhei-library.1.1 §FS-rhei-library.7.2
mod templates_selection_tests {
    use super::super::*;

    /// Template lookup walks the user tier and needs a home directory even
    /// for a built-in block. Windows CI runs `cargo test` without `HOME`, so
    /// point it at a scratch directory when it is absent; a set value is kept.
    fn ensure_test_home() {
        if std::env::var_os("HOME").is_none() {
            let home = std::env::temp_dir().join("rhei-unit-test-home");
            std::fs::create_dir_all(&home).expect("test home");
            std::env::set_var("HOME", home);
        }
    }

    #[test]
    fn five_fix_modes_omit_absent_stages_and_keep_contracts_and_duties() {
        ensure_test_home();
        for (prepare, commit) in [("none", "none"), ("none", "commit"), ("worktree", "none"), ("worktree", "commit"), ("fork", "pr")] {
            for name in ["fix", "changeset-review"] {
                let mut supplied = BTreeMap::from([
                    ("fix_prepare".into(), YamlValue::String(prepare.into())),
                    ("fix_commit".into(), YamlValue::String(commit.into())),
                ]);
                if name != "fix" { supplied.insert("change_ref".into(), YamlValue::String("HEAD~3".into())); }
                let block = BlockFrontend::new().unwrap().prepare(name, None, &supplied, name).unwrap();
                for mounted in [false, true] {
                    let block = if mounted {
                        Block { name: "outer-flow".into(), source: "<mode-test>".into(), version: "1".into(), manifest: BlockManifest::default(), local: None, children: vec![("outer".into(), block.clone())] }
                    } else { block.clone() };
                    let compiled = block.compile().unwrap();
                    let output = tempfile::tempdir().unwrap();
                    write_compiled_files(output.path(), compiled.files().unwrap()).unwrap();
                    let machine = rhei_validator::StateMachine::from_yaml_file(output.path().join("states.yaml")).unwrap();
                    let prefix = if mounted { "m5_outer__" } else { "" };
                    let state = |name: &str| format!("{prefix}{name}");
                    assert_eq!(machine.states.contains_key(&state("prepare-workspace")), prepare != "none");
                    assert_eq!(machine.states.contains_key(&state("commit-fix")), commit != "none");
                    let all_artifacts = machine.states.values().flat_map(|s| s.inputs.iter().chain(&s.outputs)).collect::<Vec<_>>();
                    assert_eq!(all_artifacts.iter().any(|a| a.name == "workspace-ref"), prepare != "none");
                    assert_eq!(all_artifacts.iter().any(|a| a.name == "commit-ref"), commit != "none");
                    let entry = state(if prepare == "none" { "final-fix" } else { "prepare-workspace" });
                    if name == "fix" { assert_eq!(compiled.entry, entry); }
                    else { assert!(machine.transitions.iter().any(|r| r.from.0 == state("human-review") && r.to.0 == entry)); }
                    let next = state(if commit == "none" { "completed" } else { "commit-fix" });
                    assert!(machine.transitions.iter().any(|r| r.from.0 == state("final-fix") && r.to.0 == next));
                    let fix = &machine.states[&state("final-fix")];
                    assert!(fix.inputs.iter().any(|a| a.name == "final-decision" && !a.optional));
                    assert!(fix.outputs.iter().any(|a| a.name == "final-fix-note" && !a.optional));
                    let prompt = machine.effective_instructions(fix).unwrap();
                    assert!(prompt.contains("Accepted Fixes") && prompt.contains("input.final-decision.path"));
                    if prepare != "none" {
                        let prep = &machine.states[&state("prepare-workspace")];
                        let prompt = machine.effective_instructions(prep).unwrap();
                        assert!(prompt.contains(if prepare == "fork" { "fork" } else { "worktree" }));
                        assert!(prep.outputs.iter().any(|a| a.name == "workspace-ref" && !a.optional));
                        assert_eq!(prep.inputs.iter().find(|a| a.name == "final-decision").unwrap().path, fix.inputs.iter().find(|a| a.name == "final-decision").unwrap().path);
                    }
                    if commit != "none" {
                        let commit_state = &machine.states[&state("commit-fix")];
                        let prompt = machine.effective_instructions(commit_state).unwrap();
                        assert!(prompt.contains(if commit == "pr" { "open a pull request" } else { "one local commit" }));
                        assert!(commit_state.outputs.iter().any(|a| a.name == "commit-ref" && !a.optional));
                    }
                    if name == "changeset-review" {
                        let source = &machine.states[&state("decide")].outputs[0].path;
                        assert_eq!(source, &fix.inputs.iter().find(|a| a.name == "final-decision").unwrap().path);
                    }
                }
            }
        }
    }

    #[test]
    fn selected_declarations_reject_unsupported_fields_types_and_static_duplicates() {
        ensure_test_home();
        let template = materialize_builtin_template("fix").unwrap();
        let original = load_template_manifest(template.path()).unwrap();
        let values = collect_template_inputs(&original, "fix", &[], &[], &[], &[]).unwrap();
        for selected in ["inputs: []", "use: []", "seams: []", "null", "[]", "ports: {entry: work, exits: {done: done}, guard: yes}", "data: {outputs: {x: {kind: state-file, state: work, name: x, expression: 1}}}", "compatibility: {unknown: {}}", "ports: {{missing}}"] {
            let mut manifest = original.clone();
            manifest.select = Some(selected.into());
            let error = select_block_declarations(&manifest, &values, template.path(), "fix").unwrap_err();
            let message = format!("{error:?}");
            assert!(message.contains("template.yaml") && message.contains("select"), "{message}");
        }
        let mut manifest = original.clone();
        manifest.static_declarations.insert("ports".into());
        let error = select_block_declarations(&manifest, &values, template.path(), "fix").unwrap_err();
        assert!(format!("{error:?}").contains("both static and selected"));
        let mut manifest = original;
        manifest.select = Some("ports: {entry: missing, exits: {done: completed}}".into());
        let selected = select_block_declarations(&manifest, &values, template.path(), "fix").unwrap();
        let mut frontend = BlockFrontend::new().unwrap();
        let block = frontend.prepare_inner(template.path(), "fix", &selected, &values).unwrap();
        let error = block.compile().unwrap_err();
        assert!(error.contains("missing") && error.contains("template.yaml"), "{error}");
    }
}
