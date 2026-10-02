// Declared writers and readers, independent of union placement and rendering.
// §FS-rhei-library.7.2
mod templates_union_artifact_tests {
    use super::super::*;

    /// A path listed repeatedly in both lists is one writer, not a reader.
    /// §FS-rhei-library.7.2
    #[test]
    fn both_lists_count_as_one_writer() {
        let machine = serde_yaml::from_str(
            r#"states:
  produce:
    inputs:
      - path: runtime/note.md
      - path: runtime/note.md
    outputs:
      - path: runtime/note.md
      - path: runtime/note.md
"#,
        )
        .unwrap();
        let paths = classify_artifact_paths(&machine);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths["runtime/note.md"].writers, BTreeSet::from(["produce".to_owned()]));
        assert!(paths["runtime/note.md"].readers.is_empty());
        assert_eq!(rhei_scoped_paths(&machine)["produce"].len(), 1);
    }

    /// Both task id spellings exclude paths from both artifact diagnostics,
    /// including the two-tickets warning's state/path view.
    /// §FS-rhei-library.7.2 §FS-rhei-library.7.2.5
    #[test]
    fn task_id_local_paths_are_per_task() {
        let machine = serde_yaml::from_str(
            r#"states:
  produce:
    outputs:
      - path: runtime/{task_id_local}.md
      - path: runtime/{task_id}.md
  read:
    inputs:
      - path: runtime/{task_id_local}.md
"#,
        )
        .unwrap();
        assert!(classify_artifact_paths(&machine).is_empty());
        assert!(rhei_scoped_paths(&machine).is_empty());
        assert!(shared_input_warnings(&machine).is_empty());
    }

    /// Readers are distinct states, sorted regardless of declaration order;
    /// optional readers participate and warning paths are sorted too.
    /// §FS-rhei-library.7.2.3
    #[test]
    fn readers_are_deduplicated_and_sorted() {
        let machine = serde_yaml::from_str(
            r#"states:
  zeta:
    inputs:
      - path: runtime/z.md
      - path: runtime/a.md
      - path: runtime/a.md
  alpha:
    inputs:
      - path: runtime/a.md
        optional: true
      - path: runtime/z.md
"#,
        )
        .unwrap();
        let paths = classify_artifact_paths(&machine);
        assert_eq!(
            paths["runtime/a.md"].readers.iter().map(String::as_str).collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );
        assert_eq!(
            shared_input_warnings(&machine),
            [
                "warning: shared input 'runtime/a.md' has no declared output producer; readers: \
             alpha, zeta; it may be supplied by a program, callback, operator or instructions.",
                "warning: shared input 'runtime/z.md' has no declared output producer; readers: \
             alpha, zeta; it may be supplied by a program, callback, operator or instructions.",
            ]
        );
    }
}
