mod templates_blocks_tests {
    use super::super::*;

    /// Both reusable blocks and the compatibility wrapper must load with their
    /// real default targets; prompt bindings retain tokens for runtime expansion.
    /// §FS-rhei-library.7 §FS-rhei-states.4.4
    #[test]
    fn extracted_builtin_prompts_load_at_identity_and_mounted_boundaries() {
        for name in ["code-review", "fix", "changeset-review"] {
            let mut frontend = BlockFrontend::new().unwrap();
            let supplied = if name == "fix" {
                BTreeMap::new()
            } else {
                BTreeMap::from([("change_ref".into(), YamlValue::String("HEAD~3".into()))])
            };
            let block = frontend.prepare(name, None, &supplied, "").unwrap();
            for mounted in [false, true] {
                let block = if mounted {
                    Block {
                        name: "mounted".into(),
                        source: "<test>".into(),
                        version: "1".into(),
                        manifest: BlockManifest::default(),
                        local: None,
                        children: vec![("outer".into(), block.clone())],
                    }
                } else {
                    block.clone()
                };
                let output = tempfile::tempdir().unwrap();
                write_compiled_files(output.path(), block.compile().unwrap().files().unwrap()).unwrap();
                let machine = rhei_validator::StateMachine::from_yaml_file(output.path().join("states.yaml")).unwrap();
                for (state_name, state) in &machine.states {
                    let prompt = machine.effective_instructions(state).unwrap();
                    assert!(prompt.contains("{task_id}"), "{name}/{state_name}: {prompt}");
                    assert!(!prompt.contains("{state}"), "{name}/{state_name}: {prompt}");
                    assert!(prompt.contains("actual mounted paths"));
                }
                if name == "changeset-review" {
                    let task = if mounted { "tasks/m5_outer__/01-coordinate.md" } else { "tasks/01-coordinate.md" };
                    assert!(output.path().join(task).is_file());
                }
            }
        }
    }

    /// Compiled content is copied once, including dot directories and literal
    /// input delimiters. §AR-rhei-library.3 §FS-rhei-library.4
    #[test]
    fn materialize_compiled_bytes_preserves_private_files_and_literals() {
        let dir = tempfile::tempdir().unwrap();
        let files: BTreeMap<PathBuf, CompiledFile> = BTreeMap::from([
            (PathBuf::from("tasks/job.md"), b"literal {{not_an_input}}".to_vec().into()),
            (PathBuf::from(".agent-grounds/rhei/settings.json"), b"{}".to_vec().into()),
            (PathBuf::from(".agent-grounds/rhei/blocks/m1_a__/scripts/tool.sh"), b"echo hello".to_vec().into()),
        ]);
        write_compiled_files(dir.path(), files.clone()).unwrap();
        for (path, bytes) in files { assert_eq!(fs::read(dir.path().join(path)).unwrap(), bytes.bytes); }
    }

    /// Unsupported guarded seams cannot silently become unconditional links.
    /// §FS-rhei-library.2 §FS-rhei-library.8
    #[test]
    fn manifest_seam_conditions_are_rejected_before_rendering() {
        let text = "name: x\nversion: 1\ndescription: x\nports: {entry: a.entry, exits: {done: b.done}}\nseams: [{from: a.done, to: b.entry, condition: false}]\n";
        let error = serde_yaml::from_str::<TemplateManifest>(text).unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }
}
