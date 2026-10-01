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
}
