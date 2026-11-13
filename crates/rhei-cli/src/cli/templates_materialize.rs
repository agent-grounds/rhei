    // Writing the output tree: the walk that copies a template directory
    // into a workspace, and what counts as text on the way through.
    // §FS-rhei-templates.6.1.2

    /// Where a materialization writes, and how a destination inside it may be
    /// named in an error. A `--dry-run` temp directory is removed before the
    /// user could look at it, so its paths must not appear. §FS-rhei-errors.4
    struct MaterializeTarget<'a> {
        root: &'a Path,
        /// True when `root` is scratch space the user neither chose nor keeps.
        scratch: bool,
        /// The template as the user named it, so a render diagnostic can say
        /// how to list the inputs it declares. §FS-rhei-templates.5.3
        template_ref: &'a str,
    }

    impl MaterializeTarget<'_> {
        /// A diagnostic for a filesystem failure at `dest`.
        fn io_report(&self, dest: &Path, action: &str, err: std::io::Error) -> Report {
            if !self.scratch {
                return file_io_report(dest, action, err);
            }
            // Name the file by its place in the template, which the user can
            // open, rather than by a temp path that no longer exists. The root
            // itself has no such place, so it is named for what it is.
            let relative = dest.strip_prefix(self.root).unwrap_or(dest);
            let named = if relative.as_os_str().is_empty() {
                "the scratch output directory".to_string()
            } else {
                format!("'{}'", relative.display())
            };
            miette!(
                help = "--dry-run renders into a temp directory. Check that $TMPDIR exists, \
                        is writable, and has free space.",
                "{action} {named} while rendering the template: {err}"
            )
        }
    }

    fn materialize_template(
        template_dir: &Path,
        template_ref: &str,
        layout: TemplateLayout,
        output_dir: &Path,
        values: &BTreeMap<String, serde_json::Value>,
        scratch: bool,
    ) -> MietteResult<MaterializedTemplate> {
        let target = MaterializeTarget { root: output_dir, scratch, template_ref };
        fs::create_dir_all(output_dir)
            .map_err(|err| target.io_report(output_dir, "failed to create output directory", err))?;
        let root_permissions = fs::metadata(template_dir)
            .map_err(|err| file_io_report(template_dir, "failed to read template metadata", err))?
            .permissions();
        fs::set_permissions(output_dir, root_permissions).map_err(|err| {
            target.io_report(output_dir, "failed to preserve output directory permissions", err)
        })?;

        materialize_template_dir(template_dir, output_dir, template_dir, values, &target)?;

        Ok(MaterializedTemplate { layout, output_dir: output_dir.to_path_buf() })
    }

    fn materialize_template_dir(
        src_dir: &Path,
        dest_dir: &Path,
        template_root: &Path,
        values: &BTreeMap<String, serde_json::Value>,
        target: &MaterializeTarget<'_>,
    ) -> MietteResult<()> {
        let mut entries = fs::read_dir(src_dir)
            .map_err(|err| file_io_report(src_dir, "failed to read template directory", err))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| miette!(
                help = "check that the template directory is readable.",
                "failed to read dir entry in '{}': {err}", src_dir.display()
            ))?;
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }

            let src_path = entry.path();
            if src_path == template_root.join("template.yaml") {
                continue;
            }

            // §FS-rhei-templates.6.1.2: root settings become project settings
            // in rhei's project-local home; non-root `settings.json` stays put.
            // Rhei writes the current home only. §FS-rhei-templates.1.1
            let at_template_root = src_dir == template_root;
            let dest_path = if at_template_root && name_str == TEMPLATE_SETTINGS_FILE {
                // The rule a refusal maps the rendering back by. §FS-rhei-errors.4.1
                let settings_path = rendered_template_settings(dest_dir);
                let settings_dir = settings_path.parent().unwrap_or(dest_dir).to_path_buf();
                fs::create_dir_all(&settings_dir).map_err(|err| {
                    target.io_report(
                        &settings_dir,
                        "failed to create the .agent-grounds/rhei directory",
                        err,
                    )
                })?;
                settings_path
            } else {
                dest_dir.join(&name)
            };
            let metadata = entry.metadata().map_err(|err| {
                file_io_report(&src_path, "failed to read template metadata", err)
            })?;

            if metadata.is_dir() {
                fs::create_dir_all(&dest_path).map_err(|err| {
                    target.io_report(&dest_path, "failed to create output directory", err)
                })?;
                fs::set_permissions(&dest_path, metadata.permissions()).map_err(|err| {
                    target.io_report(&dest_path, "failed to preserve directory permissions", err)
                })?;
                materialize_template_dir(&src_path, &dest_path, template_root, values, target)?;
                continue;
            }

            if is_text_template_file(&src_path)? {
                let raw = fs::read_to_string(&src_path).map_err(|err| {
                    file_io_report(&src_path, "failed to read template text file", err)
                })?;
                let rendered =
                    render_template_text(&raw, values, &src_path, target.template_ref)?;
                // Template-shipped settings.json must parse as JSON after
                // instantiation-variable substitution. Catching this here
                // surfaces malformed bundles before `rhei validate` runs.
                if at_template_root && name_str == "settings.json" {
                    serde_json::from_str::<serde_json::Value>(&rendered).map_err(|err| {
                        miette!(
                            help = "the template's settings.json is malformed once inputs are substituted. Fix the template file, then re-run.",
                            "template settings.json is not valid JSON after instantiation: {err}"
                        )
                    })?;
                }
                fs::write(&dest_path, rendered).map_err(|err| {
                    target.io_report(&dest_path, "failed to write output file", err)
                })?;
            } else {
                fs::copy(&src_path, &dest_path)
                    .map_err(|err| target.io_report(&dest_path, "failed to copy into", err))?;
            }

            fs::set_permissions(&dest_path, metadata.permissions()).map_err(|err| {
                target.io_report(&dest_path, "failed to preserve file permissions", err)
            })?;
        }

        Ok(())
    }

    fn is_text_template_file(path: &Path) -> MietteResult<bool> {
        let bytes = fs::read(path)
            .map_err(|err| file_io_report(path, "failed to read template file", err))?;
        Ok(!bytes[..bytes.len().min(8192)].contains(&0))
    }
