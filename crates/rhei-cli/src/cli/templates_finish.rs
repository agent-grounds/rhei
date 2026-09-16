    // Shared transactional placement, settings preparation, validation,
    // publication and execution. §FS-rhei-library.4 §FS-rhei-templates.6.2
    #[allow(clippy::too_many_arguments)]
    fn finish_template_instantiation(
        mut materialized: MaterializedTemplate, output_dir: &Path, target_dir: &Path,
        prospective_member: bool, name: &str, template: &str,
        template_input_args: &[String], set_values: &[String], set_files: &[String],
        values_files: &[PathBuf], execute: bool, dry_run: bool, keep_on_error: bool,
        execute_args: &[String],
    ) -> MietteResult<()> {
        let staged_entrypoint = materialized.entrypoint();
        let staged_state_machine_path = materialized.state_machine_path();

        // Place relative to the owning project before validating. The
        // template's machine needs no reconciling: a member rhei's own
        // declaration overrides the project default. §FS-rhei-templates.6.2

        let placement = match plan_project_placement(output_dir, &materialized.output_dir) {
            Ok(placement) => placement,
            Err(err) => {
                if !keep_on_error {
                    let _ = remove_path(target_dir, false);
                } else if prospective_member {
                    publish_staged_member(target_dir, output_dir, None, true)?;
                }
                return Err(err);
            }
        };

        let mut prepared_settings = None;
        if !dry_run {
            if let Some(project) = placement.project() {
                match prepare_workspace_settings_for_project(&materialized.output_dir, project) {
                    Ok(prepared) => prepared_settings = prepared,
                    Err(err) => {
                        if !keep_on_error {
                            let _ = remove_path(target_dir, false);
                        } else if prospective_member {
                            publish_staged_member(target_dir, output_dir, None, true)?;
                        }
                        return Err(err);
                    }
                }
            }
        }

        // A member rhei is only correct in the project's terms — its machine and
        // settings resolve there — so that is what gets validated. Validating
        // the workspace in isolation is what let a project-breaking result be
        // reported as "Validation succeeded".
        let validation = match placement.project() {
            Some(project) if !dry_run => validate_staged_project_member(
                project,
                output_dir,
                &materialized.output_dir,
                prepared_settings.is_some(),
            ),
            _ => run_validation_once(&staged_entrypoint, staged_state_machine_path.as_deref()),
        };
        if let Err(err) = validation {
            if keep_on_error && prospective_member {
                publish_staged_member(
                    target_dir, output_dir, prepared_settings.as_ref(), true,
                )?;
            } else if !keep_on_error {
                let _ = remove_path(target_dir, false);
            }
            return Err(err);
        }

        if dry_run {
            println!(
                "Dry run OK: '{}' would be instantiated into '{}'.",
                name,
                display_path(output_dir).display()
            );
            print_instantiated_workspace_summary(
                &materialized,
                output_dir,
                staged_state_machine_path.as_deref(),
                true,
            )?;
            print_template_instantiation_command(
                template,
                template_input_args,
                set_values,
                set_files,
                values_files,
                output_dir,
            );
            return Ok(());
        }

        if prospective_member {
            // No-replace rename is the only publication point. §FS-rhei-templates.6.1.2
            publish_staged_member(
                target_dir, output_dir, prepared_settings.as_ref(), keep_on_error,
            )?;
            materialized.output_dir = output_dir.to_path_buf();
        }

        let entrypoint = materialized.entrypoint();
        let state_machine_path = materialized.state_machine_path();

        println!(
            "Instantiated template '{}' into '{}'.",
            name,
            display_path(output_dir).display()
        );
        report_project_placement(&placement, prepared_settings.as_ref());
        if matches!(placement, ProjectPlacement::Standalone) {
            report_standalone_versioning(output_dir);
        }
        print_instantiated_workspace_summary(
            &materialized,
            output_dir,
            state_machine_path.as_deref(),
            false,
        )?;
        print_template_instantiation_command(
            template,
            template_input_args,
            set_values,
            set_files,
            values_files,
            output_dir,
        );

        if execute {
            let opts = parse_execute_run_options(&entrypoint, execute_args)?;
            return run_command(&entrypoint, state_machine_path.as_deref(), opts);
        }

        Ok(())
    }
