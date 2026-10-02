    // Laying one rhei from a template: rendered, staged beside its destination
    // when it joins a project, validated in the terms it will run in, and
    // published once. Every caller that lays a rhei takes this one path — the
    // default `--output`, `--into <project>` with a plan template, and each
    // `includes:` entry of a project template — so they cannot drift apart.

    // §FS-rhei-templates.6.2 §FS-rhei-library.2.2 §FS-rhei-templates.6.4

    /// One rhei to lay: the template, the values it renders with, and where it
    /// goes.
    struct MemberLay<'a> {
        template_dir: &'a Path,
        /// The template as the caller named it, which is how a render
        /// diagnostic says to list its inputs. §FS-rhei-templates.5.3
        template_ref: &'a str,
        manifest: &'a TemplateManifest,
        layout: TemplateLayout,
        values: &'a BTreeMap<String, serde_json::Value>,
        output_dir: &'a Path,
        dry_run: bool,
        keep_on_error: bool,
    }

    /// A rhei that was laid — or, under `--dry-run`, that would be — and what
    /// laying it did to the project it joined.
    struct LaidRhei {
        materialized: MaterializedTemplate,
        placement: ProjectPlacement,
        settings: Option<PreparedProjectSettings>,
        /// Computed after validation, emitted only when the whole instantiation
        /// succeeds, including a project's later members. §FS-rhei-library.7.2.3
        shared_inputs: Vec<String>,
        /// The `--dry-run` render, kept until the summary has read it.
        _scratch: Option<tempfile::TempDir>,
    }

    /// Lay one rhei at `lay.output_dir`, which must not exist yet.
    ///
    /// A Directory Workspace that joins a project is rendered into a hidden
    /// sibling, validated as the project member it will be, and published by
    /// one no-replace rename; anything else is rendered in place. Nothing is
    /// published unless it validates. §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.2
    fn lay_member_rhei(lay: &MemberLay<'_>) -> MietteResult<LaidRhei> {
        if !lay.dry_run {
            return lay_member_rhei_at(lay, None);
        }
        let scratch = tempfile::tempdir().map_err(|err| miette!(
            help = "--dry-run renders into a temp directory. Check that $TMPDIR exists and is writable.",
            "failed to create temporary output directory: {err}"
        ))?;
        // A `--dry-run` failure names the rhei asked for, as the real run's does,
        // never the scratch it was rendered in — by its path or by the id a
        // directory's name gives it. §FS-rhei-errors.4
        let name = lay.output_dir.file_name().unwrap_or(std::ffi::OsStr::new("instantiate-output"));
        let target = scratch.path().join(name);
        let respellings = copy_respellings(&target, lay.output_dir);
        let mut laid = lay_member_rhei_at(lay, Some(&target))
            .map_err(|report| respell_report(report, &respellings))?;
        laid._scratch = Some(scratch);
        Ok(laid)
    }

    /// [`lay_member_rhei`] rendering into `scratch` under `--dry-run`.
    fn lay_member_rhei_at(lay: &MemberLay<'_>, scratch: Option<&Path>) -> MietteResult<LaidRhei> {
        let prospective_member = !lay.dry_run
            && lay.layout == TemplateLayout::Workspace
            && owning_project_of(lay.output_dir).is_some();
        let target_dir = if let Some(scratch) = scratch {
            scratch.to_path_buf()
        } else if prospective_member {
            hidden_staging_path(lay.output_dir)?
        } else {
            lay.output_dir.to_path_buf()
        };
        let discard = |target: &Path| {
            if !lay.dry_run {
                let _ = remove_path(target, false);
            }
        };

        // §FS-rhei-errors.4: a --dry-run target is scratch space the user never
        // chose and never sees, so failures there name template-relative paths.
        let mut materialized = materialize_template(
            lay.template_dir,
            lay.template_ref,
            lay.layout,
            &target_dir,
            lay.values,
            lay.dry_run,
        )
        .inspect_err(|_| discard(&target_dir))?;

        // A budget identity belongs to a run and never to a template, whether
        // the template is placed or laid standalone. §FS-rhei-library.5
        refuse_rendered_identity(&lay.manifest.name, &target_dir)
            .and_then(|()| {
                apply_includes(lay.template_dir, lay.manifest, lay.values, &target_dir, lay.layout)
            })
            .inspect_err(|_| discard(&target_dir))?;

        // What happens to a failed lay: retained for inspection at the
        // requested path when the caller asked, removed otherwise.
        let fail = |settings: Option<&PreparedProjectSettings>, err: Report| -> Report {
            if lay.keep_on_error && prospective_member {
                if let Err(published) =
                    publish_staged_member(&target_dir, lay.output_dir, settings, true)
                {
                    return published;
                }
            } else if !lay.keep_on_error {
                let _ = remove_path(&target_dir, false);
            }
            err
        };

        // Place relative to the owning project before validating. The
        // template's machine needs no reconciling: a member rhei's own
        // machine overrides the project default. §FS-rhei-templates.6.2
        let placement = plan_project_placement(lay.output_dir, &materialized.output_dir)
            .map_err(|err| fail(None, err))?;

        let mut settings = None;
        if !lay.dry_run {
            if let Some(project) = placement.project() {
                settings = prepare_workspace_settings_for_project(&materialized.output_dir, project)
                    .map_err(|err| fail(None, err))?;
            }
        }

        // A member rhei is only correct in the project's terms — its machine and
        // settings resolve there — so that is what gets validated. Validating
        // the workspace in isolation is what let a project-breaking result be
        // reported as "Validation succeeded".
        let validation = match placement.project() {
            Some(project) if !lay.dry_run => validate_staged_project_member(
                project,
                lay.output_dir,
                &materialized.output_dir,
                settings.is_some(),
            ),
            _ => run_validation_once(
                &materialized.entrypoint(),
                materialized.state_machine_path().as_deref(),
            ),
        };
        if let Err(err) = validation {
            return Err(fail(settings.as_ref(), err));
        }

        // Every include has joined; diagnose this member's own machine once.
        // Inherited defaults are diagnosed by the project lay. §FS-rhei-library.7.2.3
        let shared_inputs = shared_input_warnings_in_file(&target_dir.join("states.yaml"))
            .map_err(|err| fail(settings.as_ref(), err))?;

        if prospective_member {
            // No-replace rename is the only publication point. §FS-rhei-templates.6.1.2
            publish_staged_member(&target_dir, lay.output_dir, settings.as_ref(), lay.keep_on_error)?;
            materialized.output_dir = lay.output_dir.to_path_buf();
        }
        Ok(LaidRhei { materialized, placement, settings, shared_inputs, _scratch: None })
    }

    /// `rhei instantiate <plan-template> --into <project>`: the member the
    /// default `--output` inside the project lays, at `<project>/<template-name>`,
    /// through the same path and with the same refusals. §FS-rhei-library.2.2
    #[allow(clippy::too_many_arguments)]
    fn lay_member_into_project(
        template_dir: &Path,
        template: &str,
        manifest: &TemplateManifest,
        layout: TemplateLayout,
        values: &BTreeMap<String, serde_json::Value>,
        input_args: &[String],
        project: &Path,
        dry_run: bool,
    ) -> MietteResult<()> {
        let output_dir = project.join(&manifest.name);
        if !dry_run && output_dir.exists() {
            return Err(instantiate_output_exists_error(&output_dir, template, input_args, false));
        }
        // The hoist rewrites the project's settings, so the write is serialized
        // on the project as every other `--into` is. §FS-rhei-library.2
        let _lock = if dry_run { None } else { Some(lock_new_create(project)?) };
        let laid = lay_member_rhei(&MemberLay {
            template_dir,
            template_ref: template,
            manifest,
            layout,
            values,
            output_dir: &output_dir,
            dry_run,
            keep_on_error: false,
        })?;
        report_laid_rhei(&laid, &output_dir, &manifest.name, dry_run)
    }
