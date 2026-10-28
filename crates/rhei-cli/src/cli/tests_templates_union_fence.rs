// What a fence comment's `src:sha256:` half is taken over, its own part because
// the digest is an artifact rather than a decision: a user reads it back out of
// their own `states.yaml`. §FS-rhei-library.7.1 §REQ-cross-platform.2
mod templates_union_fence_tests {
    use super::super::*;

    /// A nested entry contributes its path spelled one way, whatever separator
    /// the host walked it with. Unnormalised, one template fences a different
    /// `src:sha256:` on Windows than on Unix, and §REQ-cross-platform.2 makes an
    /// undeclared difference in an artifact a defect on the platform that
    /// differs rather than that platform's own business.
    ///
    /// `PathBuf::from_iter` spells with the native separator, so the assertion
    /// is a real one on each platform instead of a tautology on one.
    /// §FS-rhei-library.7.1 §REQ-cross-platform.2
    #[test]
    fn a_nested_entry_digests_the_same_whatever_the_platform_separator() {
        let nested = PathBuf::from_iter(["prompt_templates", "split.md"]);
        assert_eq!(
            digest_path_component(&nested),
            "prompt_templates/split.md",
            "the digest's view of a nested entry carries no platform separator"
        );

        // A lone component is itself, so the normalisation costs the flat case
        // nothing.
        assert_eq!(digest_path_component(Path::new("template.yaml")), "template.yaml");
    }

    /// And a backslash inside one component is content, not a separator. A
    /// substitution would fold `odd\name.md` together with a directory `odd`
    /// holding `name.md` — two different templates digesting alike — which is
    /// why the helper joins `components()` rather than replacing bytes. Unix
    /// only because Windows has no such file name to defend.
    /// §FS-rhei-library.7.1
    #[cfg(unix)]
    #[test]
    fn a_backslash_in_a_name_is_content_rather_than_a_separator() {
        assert_eq!(digest_path_component(Path::new("odd\\name.md")), "odd\\name.md");
        assert_ne!(
            digest_path_component(Path::new("odd\\name.md")),
            digest_path_component(&PathBuf::from_iter(["odd", "name.md"])),
            "two different sources must not digest alike"
        );
    }

    /// The worst state wins, in the order dirty > unknown > unversioned >
    /// clean, and only the template's own files count. §FS-rhei-templates.6.1.5
    #[test]
    fn the_state_word_is_the_worst_of_the_templates_own_files() {
        assert!(SourceState::Dirty > SourceState::Unknown);
        assert!(SourceState::Unknown > SourceState::Unversioned);
        assert!(SourceState::Unversioned > SourceState::Clean);
        let file = |state| SourceFile { root: PathBuf::from("repo"), state };
        let source = SourceIdentity {
            builtin: false,
            git: Some(format!("example-org/templates@{}", "a".repeat(40))),
            template_files: vec![file(SourceState::Clean), file(SourceState::Unversioned)],
            input_files: vec![file(SourceState::Dirty)],
        };
        assert_eq!(source.state_word(), "unversioned", "inputs never enter the state word");
        assert_eq!(
            source.fence_fields(),
            format!("git:example-org/templates@{} unversioned", "a".repeat(40))
        );
        assert_eq!(
            source.source_line(),
            "Source: template repo unversioned (1 file) · inputs repo dirty (1 file)"
        );
        let builtin = SourceIdentity { builtin: true, git: None, ..source };
        assert_eq!(builtin.fence_fields(), "builtin");
        assert!(builtin.source_line().starts_with("Source: template builtin · "));
    }

    /// The `laid` fence: name, version, the source digest, `git:` and the
    /// state word, then a digest of the inputs over their canonical JSON.
    /// §FS-rhei-library.7.1 §FS-rhei-templates.6.1.5
    #[test]
    fn a_laid_fence_digests_the_inputs_canonically() {
        let manifest: TemplateManifest =
            serde_yaml::from_str("name: tiny\nversion: 1.0.0\ndescription: Probe.\n")
                .expect("manifest");
        let source = SourceIdentity {
            builtin: false,
            git: None,
            template_files: Vec::new(),
            input_files: Vec::new(),
        };
        let values: BTreeMap<String, serde_json::Value> =
            [("b".to_string(), serde_json::json!({"z": 1, "a": 2})), ("a".into(), "x".into())]
                .into_iter()
                .collect();
        let inputs = sha256_hex(br#"{"a":"x","b":{"a":2,"z":1}}"#);
        assert_eq!(
            laid_fence(&manifest, "d1", &source, &values),
            format!("# --- laid tiny 1.0.0 src:sha256:d1 clean inputs:sha256:{inputs} ---\n")
        );
    }
}
