/// Unit coverage for prospective-workspace admission and create ownership.
/// §FS-rhei-new.2.1.1 §FS-rhei-new.5.1
mod new_rhei_destination_tests {
    use super::super::*;

    fn classify(root: &Path, directory_layout: bool) -> NewRheiDestination {
        classify_new_rhei_destination(root, "billing", directory_layout).expect("classification")
    }

    #[test]
    fn a_missing_destination_is_vacant() {
        let project = tempfile::tempdir().expect("tempdir");
        assert_eq!(classify(project.path(), true), NewRheiDestination::Vacant);
    }

    #[test]
    fn an_empty_directory_is_adoptable() {
        let project = tempfile::tempdir().expect("tempdir");
        fs::create_dir(project.path().join("billing")).expect("prospective workspace");
        assert_eq!(classify(project.path(), true), NewRheiDestination::AdoptableDirectory);
    }

    #[test]
    fn an_authored_machine_bundle_is_adoptable() {
        let project = tempfile::tempdir().expect("tempdir");
        let billing = project.path().join("billing");
        fs::create_dir_all(billing.join("prompt_templates")).expect("prompt directory");
        fs::write(billing.join("states.yaml"), "not parsed here\n").expect("machine");
        fs::write(billing.join("prompt_templates/review.md"), "Review\n").expect("prompt");
        assert_eq!(classify(project.path(), true), NewRheiDestination::AdoptableDirectory);
    }

    /// A prior failed create or dry run leaves the exact empty coordination
    /// entry behind; retry must admit and reuse it.
    // §FS-rhei-new.2.1.1
    #[test]
    fn issue_95_exact_empty_index_sidecar_is_adoptable() {
        let project = tempfile::tempdir().expect("tempdir");
        let billing = project.path().join("billing");
        fs::create_dir(&billing).expect("prospective workspace");
        fs::write(billing.join("index.rhei.md.lock"), b"").expect("coordination sidecar");

        assert_eq!(classify(project.path(), true), NewRheiDestination::AdoptableDirectory);
    }

    /// Similar names and objects that cannot be the empty regular sidecar stay
    /// authored obstructions; the allowlist is exact.
    // §FS-rhei-new.2.1.1
    #[test]
    fn issue_95_nonempty_and_nonregular_index_sidecars_are_obstructions() {
        let nonempty = tempfile::tempdir().expect("tempdir");
        let billing = nonempty.path().join("billing");
        fs::create_dir(&billing).expect("prospective workspace");
        let sidecar = billing.join("index.rhei.md.lock");
        fs::write(&sidecar, b"not coordination\n").expect("nonempty sidecar");
        assert_eq!(classify(nonempty.path(), true), NewRheiDestination::Occupied(sidecar));

        let nonregular = tempfile::tempdir().expect("tempdir");
        let billing = nonregular.path().join("billing");
        let sidecar = billing.join("index.rhei.md.lock");
        fs::create_dir_all(&sidecar).expect("sidecar-shaped directory");
        assert_eq!(classify(nonregular.path(), true), NewRheiDestination::Occupied(sidecar));
    }

    #[test]
    fn prompt_templates_without_a_machine_are_occupied() {
        let project = tempfile::tempdir().expect("tempdir");
        let prompts = project.path().join("billing/prompt_templates");
        fs::create_dir_all(&prompts).expect("prompt directory");
        assert_eq!(classify(project.path(), true), NewRheiDestination::Occupied(prompts));
    }

    #[test]
    fn actual_rheis_collide_in_both_requested_layouts() {
        let single = tempfile::tempdir().expect("tempdir");
        let single_path = single.path().join("billing.rhei.md");
        fs::write(&single_path, "# Rhei: Existing\n").expect("single-file rhei");
        for directory_layout in [false, true] {
            assert_eq!(
                classify(single.path(), directory_layout),
                NewRheiDestination::ExistingRhei(single_path.clone())
            );
        }

        let workspace = tempfile::tempdir().expect("tempdir");
        let workspace_path = workspace.path().join("billing");
        fs::create_dir(&workspace_path).expect("workspace");
        fs::write(workspace_path.join("index.rhei.md"), "# Rhei: Existing\n")
            .expect("workspace index");
        for directory_layout in [false, true] {
            assert_eq!(
                classify(workspace.path(), directory_layout),
                NewRheiDestination::ExistingRhei(workspace_path.clone())
            );
        }
    }

    #[test]
    fn an_adoptable_directory_conflicts_with_single_file_layout() {
        let project = tempfile::tempdir().expect("tempdir");
        let billing = project.path().join("billing");
        fs::create_dir(&billing).expect("prospective workspace");
        fs::write(billing.join("states.yaml"), "authored\n").expect("machine");
        assert_eq!(
            classify(project.path(), false),
            NewRheiDestination::LayoutConflict { directory: billing, obstruction: None }
        );
    }

    #[test]
    fn unrelated_workspace_content_names_the_obstruction() {
        let project = tempfile::tempdir().expect("tempdir");
        let notes = project.path().join("billing/notes.md");
        fs::create_dir(notes.parent().expect("parent")).expect("prospective workspace");
        fs::write(&notes, "authored notes\n").expect("notes");
        assert_eq!(classify(project.path(), true), NewRheiDestination::Occupied(notes));
    }

    #[test]
    fn ownership_excludes_a_pre_existing_root() {
        let project = tempfile::tempdir().expect("tempdir");
        let root = project.path().join("billing");
        let tasks = root.join("tasks");
        fs::create_dir(&root).expect("prospective workspace");
        fs::write(root.join("states.yaml"), "authored\n").expect("machine");
        assert_eq!(
            invocation_created_directories(&[root, tasks.clone()]).expect("ownership"),
            vec![tasks]
        );
    }

    #[test]
    fn ownership_includes_every_directory_the_invocation_will_create() {
        let project = tempfile::tempdir().expect("tempdir");
        let root = project.path().join("billing");
        let tasks = root.join("tasks");
        assert_eq!(
            invocation_created_directories(&[root.clone(), tasks.clone()]).expect("ownership"),
            vec![root, tasks]
        );
    }
}
