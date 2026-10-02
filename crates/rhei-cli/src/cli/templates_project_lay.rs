    // Laying a Panta project from a project template: the default machine, the
    // bundle it runs, the settings it needs and one member per `includes:`
    // entry — onto a new directory with `--output`, or onto a project that
    // already exists with `--into`. One renderer and one writer, told apart only
    // by whether the project is already there.

    // §FS-rhei-templates.6.4 §FS-rhei-library.2.2

    /// One `includes:` entry of a project template: the member rhei it lays at
    /// `<project>/<name>`, a member being a directory named for its rhei id.
    /// §AR-rhei-panta.1
    struct ProjectMember {
        resolved: ResolvedTemplate,
        manifest: TemplateManifest,
    }

    /// Resolve every entry of a project template to the member it lays, before
    /// anything is rendered. `under:` is refused naming the entry: there is no
    /// host task tree to place under, and inventing one would be a second
    /// placement mechanism. §FS-rhei-library.6.1 §FS-rhei-templates.6.4
    fn project_members(
        template_dir: &Path,
        manifest: &TemplateManifest,
    ) -> MietteResult<Vec<ProjectMember>> {
        let mut members: Vec<ProjectMember> = Vec::new();
        for entry in &manifest.includes {
            if let Some(under) = entry.under() {
                return Err(miette!(
                    help = "drop `under:` from the entry. Each entry of a project template is a \
                            member rhei of its own, so there is no task tree to place it under; \
                            an edge between members is an ordinary cross-rhei `**Prior:**` line \
                            in their tickets.",
                    "`includes:` entry '{}' of project template '{}' says `under: {under}`, but \
                     a project template's entries are members, not placements",
                    entry.template(),
                    manifest.name
                ));
            }
            let resolved = resolve_include(template_dir, entry.template())?;
            let included = load_template_manifest(resolved.path())?;
            // Discovery counts Directory Workspaces, so that is what a member
            // is; anything else would be laid where the project never lists it.
            let layout = detect_template_layout(resolved.path())?;
            if layout != TemplateLayout::Workspace {
                return Err(miette!(
                    help = "include a template carrying `index.rhei.md` and `tasks/`: a member \
                            is a Directory Workspace whose directory name is its rhei id.",
                    "`includes:` entry '{}' of project template '{}' is a {} template, which \
                     cannot be a member of the project",
                    entry.template(),
                    manifest.name,
                    layout.as_str()
                ));
            }
            if members.iter().any(|member| member.manifest.name == included.name) {
                return Err(miette!(
                    help = "a member's directory is its rhei id, so each template lays one \
                            member: list the entry once.",
                    "project template '{}' includes '{}' twice",
                    manifest.name,
                    included.name
                ));
            }
            members.push(ProjectMember { resolved, manifest: included });
        }
        Ok(members)
    }

    /// A project template about to be laid, and how.
    struct ProjectLay<'a> {
        template_dir: &'a Path,
        /// The template as the caller named it. §FS-rhei-templates.5.3
        template_ref: &'a str,
        manifest: &'a TemplateManifest,
        values: &'a BTreeMap<String, serde_json::Value>,
        members: &'a [ProjectMember],
        dry_run: bool,
    }

    /// A project template rendered into scratch space with the collected
    /// inputs, before any of it is laid. §FS-rhei-templates.6.4
    struct RenderedProject {
        _scratch: tempfile::TempDir,
        root: PathBuf,
        /// The `name:` of the machine it lays as the project default.
        machine: String,
    }

    /// Render the project template, exactly as §FS-rhei-templates.6.1.2 steps
    /// 3–5 render any other. Its `states.yaml` is the whole of the default it
    /// lays, so a project template without one is refused. §FS-rhei-templates.6.4
    fn render_project(lay: &ProjectLay<'_>) -> MietteResult<RenderedProject> {
        let scratch = tempfile::tempdir().map_err(|err| {
            miette!(
                help = "a project template is rendered into a temp directory before it is \
                        laid. Check that $TMPDIR exists and is writable.",
                "failed to create the render directory: {err}"
            )
        })?;
        let root = scratch.path().join(&lay.manifest.name);
        materialize_template(
            lay.template_dir,
            lay.template_ref,
            TemplateLayout::Project,
            &root,
            lay.values,
            true,
        )?;
        let machine_path = root.join("states.yaml");
        if !machine_path.is_file() {
            return Err(miette!(
                help = "add the machine the project runs under as `states.yaml` beside the \
                        template's `index.panta.md`.",
                "project template '{}' carries no states.yaml, so it has no default machine \
                 to lay",
                lay.manifest.name
            ));
        }
        let machine = machine_name_of(&read_text(&machine_path)?).ok_or_else(|| {
            miette!(
                help = "a state machine names itself with a top-level `name:`.",
                "the states.yaml of project template '{}' declares no machine name",
                lay.manifest.name
            )
        })?;
        Ok(RenderedProject { _scratch: scratch, root, machine })
    }

    /// The top-level `name:` a states file declares.
    fn machine_name_of(text: &str) -> Option<String> {
        serde_yaml::from_str::<YamlValue>(text)
            .ok()?
            .get("name")?
            .as_str()
            .map(str::to_owned)
    }

    /// What laying a project's root writes, planned against the project's own
    /// path so every message names the project the caller will see.
    struct ProjectRoot {
        writes: UnionWrites,
        /// Each top-level bundle entry copied, as the summary names it.
        copied: Vec<String>,
        /// Each file the project already has whose bytes or mode the copy
        /// changes, by its `/`-separated path in the project.
        replaced: Vec<String>,
    }

    /// The default machine whole, the bundle copied beside it, and the bundled
    /// settings hoisted into the project's home. The machine and the
    /// `prompt_templates/` and `scripts/` it runs are replaced, never unioned
    /// into, and every file of the project's that the copy changes is named
    /// rather than lost silently; another bundled file keeps the plan
    /// template's rule, that a file the project already has must be the same
    /// file. §FS-rhei-templates.6.4 §FS-rhei-library.2.2 §FS-rhei-library.2.3
    /// §FS-rhei-library.3.1
    fn plan_project_root(rendered: &Path, project: &Path) -> MietteResult<ProjectRoot> {
        let mut writes = UnionWrites::default();
        writes
            .files
            .push((project.join("states.yaml"), read_text(&rendered.join("states.yaml"))?));
        let mut copied = Vec::new();
        let mut changed = Vec::new();
        let mut entries: Vec<PathBuf> = fs::read_dir(rendered)
            .map_err(|err| file_io_report(rendered, "failed to read the rendered template", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            // The settings travel by the hoist below, from the hidden home
            // the render moved them to. §FS-rhei-templates.6.2
            if name.starts_with('.')
                || [workspace::PANTA_INDEX_FILE, "states.yaml", "README.md"].contains(&name)
            {
                continue;
            }
            let replaced = name == "prompt_templates" || name == "scripts";
            let mut files = Vec::new();
            bundle_files(&entry, &mut files)?;
            for src in &files {
                let relative = src.strip_prefix(rendered).unwrap_or(src);
                let dst = project.join(relative);
                if dst.is_file() {
                    let bytes_differ = fs::read(&dst).ok() != fs::read(src).ok();
                    if !replaced && bytes_differ {
                        return Err(miette!(
                            help = "a project template renames nothing: give the template's copy \
                                    a name of its own.",
                            "'{}' is shipped by the template and already exists in the project \
                             with different contents",
                            display_slash(&dst)
                        ));
                    }
                    if bytes_differ || permissions_of(&dst) != permissions_of(src) {
                        changed.push(display_slash(relative));
                    }
                }
                writes.copies.push((src.clone(), dst));
            }
            copied.push(if entry.is_dir() {
                format!("{name}/ ({})", plural(files.len(), "file"))
            } else {
                name.to_owned()
            });
        }
        if let Some(found) = resolve_rhei_home_file(rendered, WORKSPACE_SETTINGS_FILE) {
            hoist_settings_into_project(found.path(), project, &mut writes)?;
        }
        Ok(ProjectRoot { writes, copied, replaced: changed })
    }

    fn permissions_of(path: &Path) -> Option<fs::Permissions> {
        fs::metadata(path).ok().map(|meta| meta.permissions())
    }

    /// Every file under `path`, in a stable order.
    fn bundle_files(path: &Path, files: &mut Vec<PathBuf>) -> MietteResult<()> {
        if !path.is_dir() {
            files.push(path.to_path_buf());
            return Ok(());
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(path)
            .map_err(|err| file_io_report(path, "failed to read a template directory", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            bundle_files(&entry, files)?;
        }
        Ok(())
    }

    /// Write `writes`, planned against `planned`, beneath `root` instead: the
    /// project itself, the staging directory it is laid in, or a validation
    /// copy. A copy keeps its file's permissions, so a script stays runnable.
    fn write_project_root(writes: &UnionWrites, planned: &Path, root: &Path) -> MietteResult<()> {
        let rebase = |path: &Path| root.join(path.strip_prefix(planned).unwrap_or(path));
        for (path, contents) in &writes.files {
            write_mirrored(planned, root, path, contents.as_bytes())?;
        }
        for (src, dst) in &writes.copies {
            let dst = rebase(dst);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| file_io_report(parent, "failed to create a directory", err))?;
            }
            fs::copy(src, &dst).map_err(|err| file_io_report(src, "failed to copy", err))?;
        }
        Ok(())
    }

    /// The bytes and permissions every root write is about to replace, so a
    /// failure after the root is written puts the project back as it was — a
    /// copy carries its source's mode, so the bytes alone would leave a script
    /// with the template's. The project's settings file is always kept,
    /// because a member's hoist writes it too.
    struct RootBackup {
        project: PathBuf,
        saved: Vec<(PathBuf, Option<SavedFile>)>,
    }

    /// A file as it stood before the root write, `None` beside its path when
    /// there was none.
    struct SavedFile {
        bytes: Vec<u8>,
        permissions: Option<fs::Permissions>,
    }

    impl RootBackup {
        fn take(writes: &UnionWrites, project: &Path) -> Self {
            let mut paths: Vec<PathBuf> = writes
                .files
                .iter()
                .map(|(path, _)| path.clone())
                .chain(writes.copies.iter().map(|(_, dst)| dst.clone()))
                .collect();
            paths.push(project_settings_write_path(project));
            paths.sort();
            paths.dedup();
            let saved = paths.into_iter().map(|path| {
                let kept = fs::read(&path)
                    .ok()
                    .map(|bytes| SavedFile { bytes, permissions: permissions_of(&path) });
                (path, kept)
            });
            Self { project: project.to_path_buf(), saved: saved.collect() }
        }

        /// Put every saved path back as it was, removing what did not exist
        /// and the directories created only to hold it.
        fn restore(&self) {
            for (path, kept) in &self.saved {
                match kept {
                    Some(kept) => {
                        let _ = fs::write(path, &kept.bytes);
                        if let Some(permissions) = &kept.permissions {
                            let _ = fs::set_permissions(path, permissions.clone());
                        }
                    }
                    None => {
                        if fs::remove_file(path).is_ok() {
                            prune_empty_parents(path, &self.project);
                        }
                    }
                }
            }
        }
    }

    /// `rhei instantiate <project-template> --output <dir>`: lay a whole project
    /// at a directory that does not exist yet. Everything is laid in a hidden
    /// sibling, validated as a project, and published by one no-replace rename,
    /// so nothing is published unless the project it lays is valid.
    /// §FS-rhei-templates.6.4 §FS-rhei-templates.6.1.2
    fn lay_project_output(
        lay: &ProjectLay<'_>,
        output_dir: &Path,
        keep_on_error: bool,
    ) -> MietteResult<()> {
        let rendered = render_project(lay)?;
        let scratch = if lay.dry_run {
            Some(tempfile::tempdir().map_err(|err| miette!(
                help = "--dry-run lays the project in a temp directory. Check that $TMPDIR exists \
                        and is writable.",
                "failed to create temporary output directory: {err}"
            ))?)
        } else {
            None
        };
        let staged = match scratch.as_ref() {
            Some(scratch) => scratch.path().join(output_dir.file_name().unwrap_or_default()),
            None => hidden_staging_path(output_dir)?,
        };
        // A refusal names the project the caller asked for, not its staging.
        let laid = lay_project_staged(lay, &rendered, output_dir, &staged)
            .map_err(|err| respell_staging(err, &staged, output_dir));
        let summary = match laid {
            Ok(summary) => summary,
            Err(err) if lay.dry_run || !keep_on_error => {
                let _ = remove_path(&staged, false);
                return Err(err);
            }
            Err(err) => {
                let retained = match rename_member_noreplace(&staged, output_dir) {
                    Ok(()) => output_dir.to_path_buf(),
                    Err(_) => staged.clone(),
                };
                return Err(miette!(
                    help = format!(
                        "the laid project is retained at '{}' for inspection, because \
                         --keep-on-error was passed.",
                        display_slash(&retained)
                    ),
                    "{err}"
                ));
            }
        };
        if !lay.dry_run {
            if let Err(err) = rename_member_noreplace(&staged, output_dir) {
                let _ = remove_path(&staged, false);
                return Err(file_io_report(output_dir, "failed to publish the laid project", err));
            }
        }
        summary.print(output_dir, lay.dry_run);
        Ok(())
    }

    /// Lay the manifest, the root and the members into `staged`, planned
    /// against `output_dir`, and validate the whole as a project.
    fn lay_project_staged(
        lay: &ProjectLay<'_>,
        rendered: &RenderedProject,
        output_dir: &Path,
        staged: &Path,
    ) -> MietteResult<ProjectSummary> {
        fs::create_dir_all(staged)
            .map_err(|err| file_io_report(staged, "failed to create the project", err))?;
        let manifest = rendered.root.join(workspace::PANTA_INDEX_FILE);
        copy_tree(&manifest, &staged.join(workspace::PANTA_INDEX_FILE))?;
        let root = plan_project_root(&rendered.root, output_dir)?;
        write_project_root(&root.writes, output_dir, staged)?;
        let mut summary = ProjectSummary::new(lay.manifest, &rendered.machine, &root);
        let verb = if lay.dry_run { "would be laid" } else { "laid" };
        for member in lay.members {
            // Laid for real even under `--dry-run`: `staged` is then scratch.
            let laid = lay_project_member(lay, member, staged, false)?;
            summary.members.push(member_line(&member.manifest.name, verb, &laid));
        }
        let pass = validation_pass(staged, None)?;
        if !pass.errors.is_empty() {
            return Err(validation_report(
                staged,
                &pass.state_machine_sources,
                &pass.errors,
                &pass.help,
            ));
        }
        summary.warnings = pass.warnings;
        Ok(summary)
    }

    /// Lay one member through the one member-laying path, at `<project>/<name>`.
    /// §FS-rhei-templates.6.2
    fn lay_project_member(
        lay: &ProjectLay<'_>,
        member: &ProjectMember,
        project: &Path,
        dry_run: bool,
    ) -> MietteResult<LaidRhei> {
        lay_member_rhei(&MemberLay {
            template_dir: member.resolved.path(),
            template_ref: &member.manifest.name,
            manifest: &member.manifest,
            layout: TemplateLayout::Workspace,
            values: lay.values,
            output_dir: &project.join(&member.manifest.name),
            dry_run,
            keep_on_error: false,
        })
        .map_err(|err| {
            miette!(
                help = err.help().map_or_else(
                    || "read the refusal above against the member's template.".to_owned(),
                    |help| help.to_string(),
                ),
                "`includes:` entry '{}' of project template '{}': {err}",
                member.manifest.name,
                lay.manifest.name
            )
        })
    }

    /// The summary line for a member that was laid, naming what its settings
    /// added to the project's. §FS-rhei-templates.6.2
    fn member_line(name: &str, verb: &str, laid: &LaidRhei) -> String {
        match laid.settings.as_ref() {
            Some(settings) if !settings.added.is_empty() => {
                format!("{name} {verb}, its settings adding {}", settings.added.join(", "))
            }
            _ => format!("{name} {verb}"),
        }
    }
