    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate_command(
        template: Option<&str>,
        mounts: &[String],
        input_args: &[String],
        execute_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        seams: &[String],
        passes: &[String],
        output: Option<&Path>,
        execute: bool,
        dry_run: bool,
        keep_on_error: bool,
        list_inputs: bool,
    ) -> MietteResult<()> {
        if execute && dry_run {
            return Err(miette!(
                help = "--dry-run renders and validates without writing anything, so there is \
                        nothing for --execute to run. Drop one of them.",
                "--execute cannot be used together with --dry-run"
            ));
        }

        // Explicit mounts opt into composition; with no mount the positional
        // grammar below stays byte-for-byte the legacy template grammar.
        // §FS-rhei-library.3
        if !mounts.is_empty() {
            if template.is_some() || !input_args.is_empty() {
                return Err(miette!(
                    help = "use only repeatable `--mount alias=block` operands and qualified `--set alias.input=value` inputs.",
                    "direct block composition cannot be combined with a positional template or positional inputs"
                ));
            }
            return instantiate_direct_blocks(
                mounts,
                seams,
                passes,
                values_files,
                set_values,
                set_files,
                output,
                execute,
                dry_run,
                keep_on_error,
                list_inputs,
                execute_args,
            );
        }
        if !seams.is_empty() || !passes.is_empty() {
            return Err(miette!(help = "add --mount ALIAS=BLOCK before declaring a seam or pass", "--seam and --pass require at least one --mount"));
        }

        let Some(template) = template else {
            // §FS-rhei-templates.6.1.2: an omitted template lists available templates.
            return templates_command(false, "all", None);
        };

        let resolved_template = resolve_template_reference(template)?;
        let template_dir = resolved_template.path();
        let manifest = load_template_manifest(template_dir)?;

        // Selected interfaces take the same typed compilation path. §FS-rhei-library.1.1
        if !manifest.block.mounts.is_empty() || manifest.select.is_some() {
            let template_input_args =
                template_input_args_without_execute_args(input_args, execute_args)?;
            return instantiate_curated_block(
                template,
                template_dir,
                &manifest,
                &template_input_args,
                set_values,
                set_files,
                values_files,
                output,
                execute,
                dry_run,
                keep_on_error,
                list_inputs,
                execute_args,
            );
        }

        if list_inputs {
            print_template_inputs(&manifest, template);
            return Ok(());
        }

        let layout = detect_template_layout(template_dir)?;
        let template_input_args =
            template_input_args_without_execute_args(input_args, execute_args)?;
        let resolved_values = collect_template_inputs(
            &manifest,
            template,
            values_files,
            &template_input_args,
            set_values,
            set_files,
        )?;
        let cwd = std::env::current_dir()
            .map_err(|err| {
                miette!(
                    help = cwd_help(),
                    "failed to determine working directory: {err}"
                )
            })?;
        let template_name = template_dir.file_name().ok_or_else(|| {
            miette!(
                help = "pass an explicit destination with --output <dir>",
                "template path '{}' has no directory name",
                template_dir.display()
            )
        })?;
        // Inside a project the default home is the project itself; defaulting
        // to the working directory dropped the workspace where discovery never
        // looks, so no command listed it. §FS-rhei-templates.6.2
        let default_output =
            enclosing_project_for_new_rhei(&cwd).unwrap_or(cwd).join(template_name);
        let explicit_output = output.is_some();
        let output_dir = output.map(Path::to_path_buf).unwrap_or(default_output);

        if !dry_run && output_dir.exists() {
            return Err(instantiate_output_exists_error(
                &output_dir,
                template,
                &template_input_args,
                explicit_output,
            ));
        }

        let scratch = if dry_run {
            Some(
                tempfile::tempdir()
                    .map_err(|err| miette!(
                        help = "--dry-run renders into a temp directory. Check that $TMPDIR exists and is writable.",
                        "failed to create temporary output directory: {err}"
                    ))?,
            )
        } else {
            None
        };
        let prospective_member = !dry_run
            && layout == TemplateLayout::Workspace
            && owning_project_of(&output_dir).is_some();
        let target_dir = if let Some(scratch) = scratch.as_ref() {
            scratch.path().join("instantiate-output")
        } else if prospective_member {
            hidden_staging_path(&output_dir)?
        } else {
            output_dir.clone()
        };

        // §FS-rhei-errors.4: a --dry-run target is scratch space the user never
        // chose and never sees, so failures there name template-relative paths.
        let materialized =
            match materialize_template(
                template_dir,
                template,
                layout,
                &target_dir,
                &resolved_values,
                dry_run,
            ) {
                Ok(materialized) => materialized,
                Err(err) => {
                    if !dry_run {
                        let _ = remove_path(&target_dir, false);
                    }
                    return Err(err);
                }
            };

        finish_template_instantiation(materialized, &output_dir, &target_dir, prospective_member, &manifest.name, template, &template_input_args, set_values, set_files, values_files, execute, dry_run, keep_on_error, execute_args)
    }

    /// A standalone workspace inside a git repository is tracked content
    /// unless the user says otherwise — unlike `panta/`, which init ignores.
    // §FS-rhei-templates.6.2: note the untracked workspace; never edit
    // `.gitignore` — committed workspaces (examples) are the other use.
    fn report_standalone_versioning(output_dir: &Path) {
        let parent = match output_dir.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        let toplevel = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(parent)
            .output();
        let Ok(toplevel) = toplevel else {
            return; // no git on PATH: nothing worth guessing about
        };
        if !toplevel.status.success() {
            return; // not inside a git repository
        }
        let ignored = std::process::Command::new("git")
            .args(["check-ignore", "-q", "--"])
            .arg(output_dir.file_name().unwrap_or_default())
            .current_dir(parent)
            .status();
        match ignored {
            Ok(status) if status.code() == Some(1) => {}
            _ => return, // already ignored, or git could not tell
        }
        let repo_root = PathBuf::from(String::from_utf8_lossy(&toplevel.stdout).trim());
        let entry = rhei_core::platform::canonical_path(output_dir)
            .ok()
            .and_then(|dir| dir.strip_prefix(&repo_root).map(|rel| rel.to_path_buf()).ok())
            .unwrap_or_else(|| output_dir.to_path_buf());
        println!(
            "Note: this standalone workspace is inside a git repository and is not gitignored, \
             so its planning state (including `runtime/`) is repository content. Commit it to \
             version the workspace, or add `{}/` to .gitignore to keep it working material — \
             the stance `rhei init` takes for `panta/`.",
            entry.display()
        );
    }

    /// Render `path` for the report: relative to the working directory when it
    /// sits inside it. The report is read — and its commands pasted — from that
    /// directory, so `panta/product-management` beats the absolute spelling.
    // §FS-rhei-templates.6.1.3: report paths inside the working directory are relative.
    pub(super) fn display_path(path: &Path) -> PathBuf {
        if path.is_relative() {
            return path.to_path_buf();
        }
        let Ok(cwd) = std::env::current_dir() else {
            return path.to_path_buf();
        };
        if let Some(relative) = strip_to_relative(path, &cwd) {
            return relative;
        }
        // `current_dir` reports the resolved path, so a symlinked parent makes
        // the comparison above miss and the report falls back to absolute
        // spelling for a path that is in fact inside the working directory —
        // macOS resolves `/tmp` and `/var` into `/private`, so every report
        // written from a temporary directory there took that fallback.
        let (Ok(resolved_path), Ok(resolved_cwd)) =
            (rhei_core::platform::canonical_path(path), rhei_core::platform::canonical_path(&cwd))
        else {
            return path.to_path_buf();
        };
        strip_to_relative(&resolved_path, &resolved_cwd).unwrap_or_else(|| path.to_path_buf())
    }

    /// `path` expressed relative to `base`, or `None` when it is not under it.
    fn strip_to_relative(path: &Path, base: &Path) -> Option<PathBuf> {
        match path.strip_prefix(base) {
            Ok(rel) if rel.as_os_str().is_empty() => Some(PathBuf::from(".")),
            Ok(rel) => Some(rel.to_path_buf()),
            Err(_) => None,
        }
    }

    fn template_input_args_without_execute_args(
        input_args: &[String],
        execute_args: &[String],
    ) -> MietteResult<Vec<String>> {
        if execute_args.is_empty() {
            return Ok(input_args.to_vec());
        }
        if input_args.len() < execute_args.len() {
            return Err(miette!(
                help = internal_error_help(),
                "internal error: execute arguments were not present in parsed template inputs"
            ));
        }

        let split_at = input_args.len() - execute_args.len();
        if input_args[split_at..] != *execute_args {
            return Err(miette!(
                help = internal_error_help(),
                "internal error: execute arguments did not match trailing parsed template inputs"
            ));
        }
        Ok(input_args[..split_at].to_vec())
    }

    fn parse_execute_run_options(
        entrypoint: &Path,
        execute_args: &[String],
    ) -> MietteResult<RunOptions> {
        if execute_args.is_empty() {
            return Ok(default_run_options());
        }

        let mut args =
            vec!["rhei".to_string(), "run".to_string(), entrypoint.display().to_string()];
        args.extend(execute_args.iter().cloned());

        let cli = Cli::try_parse_from(args).map_err(|err| {
            miette!(
                help = "everything after --execute is passed to `rhei run`. See its flags with: \
                        rhei run --help",
                "{}",
                err.to_string()
            )
        })?;
        let Commands::Run { standalone, agent, program, snapshot, .. } = cli.command else {
            return Err(miette!(
                help = internal_error_help(),
                "internal error: execute arguments did not parse as run options"
            ));
        };
        Ok((standalone, agent, program, snapshot).into())
    }

    fn print_template_instantiation_command(
        template: &str,
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        output_dir: &Path,
    ) {
        println!("Instantiate this template with:");
        println!(
            "  {}",
            format_template_instantiation_command(
                template,
                input_args,
                set_values,
                set_files,
                values_files,
                Some(output_dir),
                &[],
            )
        );
    }

    /// Rebuild the `rhei instantiate` invocation the user made, plus any
    /// `extra_inputs`, so a suggestion pastes as-is. §FS-rhei-errors.1.2
    fn format_template_instantiation_command(
        template: &str,
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        output_dir: Option<&Path>,
        extra_inputs: &[String],
    ) -> String {
        let mut parts = vec!["rhei".to_string(), "instantiate".to_string(), template.to_string()];
        for values_file in values_files {
            parts.push("--values".to_string());
            parts.push(values_file.display().to_string());
        }
        parts.extend(input_args.iter().cloned());
        parts.extend(extra_inputs.iter().cloned());
        for value in set_values {
            parts.push("--set".to_string());
            parts.push(value.clone());
        }
        for value in set_files {
            parts.push("--set-file".to_string());
            parts.push(value.clone());
        }
        if let Some(output_dir) = output_dir {
            parts.push("--output".to_string());
            parts.push(display_path(output_dir).display().to_string());
        }

        // §FS-rhei-errors.2: printed commands are pasted into a shell, and a
        // selector like `codex[yolo]:openai:gpt-5.5` is a zsh glob unquoted.
        shell_command(&parts)
    }

    /// The error for `rhei instantiate` when the output directory is taken.
    ///
    /// Instantiating the same template twice is the ordinary way to review two
    /// specs, audit two subjects, or run two release checklists, and it is the
    /// most likely second thing anyone does with a template. The bare
    /// `output path '…' already exists` was a dead end at exactly that moment:
    /// it named no fix, and the reason the collision happens — the default
    /// output directory is the template's own name, and a rhei's id *is* its
    /// directory name — is invisible from the message.
    // §FS-rhei-templates.6.2
    fn instantiate_output_exists_error(
        output_dir: &Path,
        template: &str,
        input_args: &[String],
        explicit_output: bool,
    ) -> Report {
        if explicit_output {
            return miette!(
                help = format!(
                    "pass a --output path that does not exist yet, or remove that one: rm -rf {}",
                    shell_quote(&output_dir.display().to_string())
                ),
                "output path '{}' already exists",
                output_dir.display()
            );
        }
        let suggestion = relative_to_cwd(&next_free_sibling(output_dir));
        let mut parts = vec!["rhei".to_string(), "instantiate".to_string(), template.to_string()];
        parts.extend(input_args.iter().cloned());
        parts.push("--output".to_string());
        parts.push(suggestion.display().to_string());
        miette!(
            help = format!("name it for what it is about:\n  {}", shell_command(&parts)),
            "'{}' already exists, so template '{}' has already been instantiated here under \
             that name. A second copy needs its own directory, because the directory name \
             becomes the rhei id every one of its ticket ids is prefixed with.",
            output_dir.display(),
            template
        )
    }

    /// The first `<name>-<n>` beside `path` that nothing occupies, as a
    /// copy-pasteable starting point when the caller has no better name.
    fn next_free_sibling(path: &Path) -> PathBuf {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return path.to_path_buf();
        };
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        (2u32..100)
            .map(|n| parent.join(format!("{name}-{n}")))
            .find(|candidate| !candidate.exists())
            .unwrap_or_else(|| parent.join(format!("{name}-copy")))
    }

    /// `path` written relative to the working directory when it sits beneath
    /// it, so a suggested command is short enough to read and paste.
    fn relative_to_cwd(path: &Path) -> PathBuf {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| path.to_path_buf())
    }
