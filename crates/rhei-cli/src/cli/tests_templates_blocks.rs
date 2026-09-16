mod templates_blocks_tests {
    use super::super::*;

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
