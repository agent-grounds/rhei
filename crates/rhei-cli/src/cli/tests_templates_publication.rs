// Tests at the publication boundary: the competing path arrives after staging
// and validation, without a timing race or a public test hook.
// §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.2
mod templates_publication_tests {
    use super::super::*;

    fn stage(project: &Path) -> (PathBuf, PathBuf, PreparedProjectSettings, String) {
        fs::write(project.join("index.panta.md"), "# Panta: Publication\n").unwrap();
        let output = project.join("follow");
        assert!(!output.exists());
        let staged = hidden_staging_path(&output).unwrap();
        fs::create_dir_all(staged.join("tasks")).unwrap();
        fs::write(staged.join("index.rhei.md"), "# Rhei: Follow\n").unwrap();
        fs::write(staged.join("tasks/01-work.md"), "### Task 1: Work\n**State:** pending\n").unwrap();
        let settings_path = Path::new(".agent-grounds/rhei/settings.json");
        fs::create_dir_all(staged.join(settings_path).parent().unwrap()).unwrap();
        fs::write(
            staged.join(settings_path),
            r#"{"models":{"publication-fixture":{"provider":"fixture","model":"new"}}}"#,
        )
        .unwrap();
        fs::create_dir_all(project.join(settings_path).parent().unwrap()).unwrap();
        let previous = "{\"models\":{}}\n".to_string();
        fs::write(project.join(settings_path), &previous).unwrap();
        let settings = prepare_workspace_settings_for_project(&staged, project).unwrap().unwrap();
        validate_staged_project_member(project, &output, &staged, true).unwrap();
        (staged, output, settings, previous)
    }

    #[test]
    fn issue_205_publication_preserves_a_destination_created_after_validation() {
        for keep in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let project = dir.path();
            let (staged, output, settings, previous) = stage(project);
            // An ordinary Unix rename would replace this empty directory.
            fs::create_dir(&output).unwrap();
            let competing = fs::metadata(&output).unwrap();
            let err = publish_staged_member(&staged, &output, Some(&settings), keep).unwrap_err();
            assert!(err.to_string().contains("failed to publish"));
            assert!(output.is_dir());
            assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(fs::metadata(&output).unwrap().ino(), competing.ino());
            }
            #[cfg(not(unix))]
            let _ = competing;
            assert_eq!(
                fs::read_to_string(project.join(".agent-grounds/rhei/settings.json")).unwrap(),
                previous
            );
            assert_eq!(staged.exists(), keep);
            if keep {
                assert!(staged.join("index.rhei.md").is_file());
                assert!(fs::read_to_string(staged.join(".agent-grounds/rhei/settings.json"))
                    .unwrap()
                    .contains("publication-fixture"));
                assert!(format!("{err:?}").contains(&staged.display().to_string()));
            }
        }
    }

    #[test]
    fn issue_205_publication_preserves_a_competing_file() {
        let dir = tempfile::tempdir().unwrap();
        let (staged, output, settings, previous) = stage(dir.path());
        fs::write(&output, "owned by another actor\n").unwrap();
        assert!(publish_staged_member(&staged, &output, Some(&settings), false).is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), "owned by another actor\n");
        assert!(!staged.exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(".agent-grounds/rhei/settings.json")).unwrap(),
            previous
        );
    }

    #[test]
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox",
        windows
    ))]
    fn issue_205_publication_moves_a_validated_member_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let (staged, output, settings, _) = stage(dir.path());
        publish_staged_member(&staged, &output, Some(&settings), false).unwrap();
        assert!(!staged.exists());
        assert!(output.join("index.rhei.md").is_file());
        assert!(!output.join(".agent-grounds/rhei/settings.json").exists());
        assert!(fs::read_to_string(dir.path().join(".agent-grounds/rhei/settings.json"))
            .unwrap()
            .contains("publication-fixture"));
    }
}
