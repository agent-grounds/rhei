    // Compiled bytes bypass template filtering and rendering. The shared final
    // transaction still places, hoists settings, validates and rolls back.
    // §AR-rhei-library.3 §FS-rhei-library.4.1 §FS-rhei-templates.6.1.2
    #[allow(clippy::too_many_arguments)]
    fn instantiate_compiled_workspace(
        compiled: CompiledBlock, root_name: &str, output: Option<&Path>,
        execute: bool, dry_run: bool, keep_on_error: bool, execute_args: &[String],
    ) -> MietteResult<()> {
        let cwd = std::env::current_dir().map_err(|e| miette!(help = "run from a readable working directory", "current directory: {e}"))?;
        let default = enclosing_project_for_new_rhei(&cwd).unwrap_or(cwd).join(root_name);
        let output_dir = output.map(Path::to_path_buf).unwrap_or(default);
        if !dry_run && output_dir.exists() {
            return Err(instantiate_output_exists_error(&output_dir, root_name, &[], output.is_some()));
        }
        let scratch = if dry_run { Some(tempfile::tempdir().map_err(|e| miette!(help = "check that the temporary directory is writable", "compiler output scratch: {e}"))?) } else { None };
        // A compiled member is staged and published exactly like a rendered one.
        // §FS-rhei-templates.6.1.2
        let prospective_member = !dry_run && owning_project_of(&output_dir).is_some();
        let target = if let Some(scratch) = scratch.as_ref() {
            scratch.path().join("instantiate-output")
        } else if prospective_member {
            hidden_staging_path(&output_dir)?
        } else {
            output_dir.clone()
        };
        let files = compiled.files().map_err(|e| miette!(help = "fix the composed block declarations, then retry", "{e}"))?;
        if let Err(error) = write_compiled_files(&target, files) {
            if !dry_run && !keep_on_error { let _ = remove_path(&target, false); }
            return if !dry_run && keep_on_error {
                Err(error.wrap_err(format!("kept partial composed output at '{}' because --keep-on-error was passed", target.display())))
            } else {
                Err(error)
            };
        }
        finish_template_instantiation(
            MaterializedTemplate { layout: TemplateLayout::Workspace, output_dir: target.clone() },
            &output_dir, &target, prospective_member, root_name, root_name, &[], &[], &[], &[],
            execute, dry_run, keep_on_error, execute_args,
        )
    }

    fn write_compiled_files(target: &Path, files: BTreeMap<PathBuf, CompiledFile>) -> MietteResult<()> {
        for (relative, bytes) in files {
            let path = target.join(relative);
            if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| file_io_report(parent, "create compiled directory", e))?; }
            fs::write(&path, bytes.bytes).map_err(|e| file_io_report(&path, "write compiled file", e))?;
            if let Some(permissions) = bytes.permissions { fs::set_permissions(&path, permissions).map_err(|e| file_io_report(&path, "preserve compiled permissions", e))?; }
        }
        Ok(())
    }
