    // What an instantiation says once its rhei is laid: where it went, what
    // joining a project did to the project, and the summary of what it holds.
    // §FS-rhei-templates.6.1.3 §FS-rhei-templates.6.2

    /// Report a rhei `lay_member_rhei` laid, or under `--dry-run` the one it
    /// would have. The settings commit is a write outside the member, so it is
    /// never silent. §FS-rhei-templates.6.1.3 §FS-rhei-templates.6.2
    fn report_laid_rhei(
        laid: &LaidRhei,
        output_dir: &Path,
        name: &str,
        dry_run: bool,
    ) -> MietteResult<()> {
        let state_machine_path = laid.materialized.state_machine_path();
        if dry_run {
            println!(
                "Dry run OK: '{}' would be instantiated into '{}'.",
                name,
                display_path(output_dir).display()
            );
            return print_instantiated_workspace_summary(
                &laid.materialized,
                output_dir,
                state_machine_path.as_deref(),
                true,
            );
        }
        println!(
            "Instantiated template '{}' into '{}'.",
            name,
            display_path(output_dir).display()
        );
        report_project_placement(&laid.placement, laid.settings.as_ref());
        if matches!(laid.placement, ProjectPlacement::Standalone) {
            report_standalone_versioning(output_dir);
        }
        print_instantiated_workspace_summary(
            &laid.materialized,
            output_dir,
            state_machine_path.as_deref(),
            false,
        )
    }
