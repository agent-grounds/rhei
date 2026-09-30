    // What changes when the union's target is a *member* of a Panta project:
    // where its settings go, whose terms it is validated in, and what the
    // scope lock has to cover.
    //
    // Its own part because all three answers are the same one fact — a member
    // rhei is only correct in the project's terms, so the project is what the
    // hoist writes to, what the validation loads, and what the write is
    // serialized on. A union that answered any of them from the rhei alone
    // would leave a project it just broke.

    // §FS-rhei-library.2 §FS-rhei-templates.6.2 §AR-rhei-library.5

    /// The project a union's target is a member of, or `None` outside one.
    ///
    /// Discovery counts only the entries sitting directly beside
    /// `index.panta.md`, so a directory-workspace member's project is its
    /// parent and a single-file member's is the directory it sits in.
    /// §AR-rhei-panta.1
    fn union_project(root: &Path, single_file: bool) -> Option<PathBuf> {
        if single_file {
            return workspace::is_panta_project(root).then(|| root.to_path_buf());
        }
        let parent = root.parent()?;
        workspace::is_panta_project(parent).then(|| parent.to_path_buf())
    }

    /// The directory whose sidecar stands for the whole placement: the project
    /// for a member, the rhei's own root outside one.
    ///
    /// The scope lock has to span every file the placement writes, and for a
    /// member that includes the project's `settings.json`, which the hoist
    /// reads, merges and writes back. Locking the member instead would leave
    /// two unions into two members of one project racing over that one file,
    /// the loser's agents lost and the project left naming a target it no
    /// longer defines. Widening also serializes the placement against
    /// `rhei new --under <member>`, which already resolves its own scope to the
    /// project (§FS-rhei-new.1.1), and a project-wide scope is strictly broader
    /// than a member one, so sibling numbering keeps its guarantee.
    /// §FS-rhei-library.2 §FS-rhei-new.4
    fn union_scope_root(host: &UnionHost) -> &Path {
        host.project.as_deref().unwrap_or(&host.root)
    }

    /// Hoist a template's `settings.json` into the project, which is the only
    /// settings root a member rhei resolves: a copy beside the rhei is read by
    /// nothing, and the machine the union just brought names the agents it
    /// defines. Values the project already holds win, and the summary names
    /// both what was added and what was kept.
    /// §FS-rhei-library.2 §FS-rhei-templates.6.2 §FS-rhei-agents.1.1
    fn hoist_settings_into_project(
        source: &Path,
        project: &Path,
        writes: &mut UnionWrites,
    ) -> MietteResult<()> {
        let incoming = read_settings_value(source)?;
        let existing = resolve_rhei_home_file(project, WORKSPACE_SETTINGS_FILE);
        if let Some(found) = &existing {
            found.warn_if_deprecated();
        }
        let mut merged = match &existing {
            Some(found) => read_settings_value(found.path())?,
            None => serde_json::json!({}),
        };
        let (mut added, mut kept) = (Vec::new(), Vec::new());
        merge_settings_value(&mut merged, &incoming, "", &mut added, &mut kept);
        let target = project_settings_write_path(project);
        let rendered = serde_json::to_string_pretty(&merged)
            .map_err(|err| {
                miette!(help = internal_error_help(), "failed to render merged settings: {err}")
            })?
            + "\n";
        let where_to = display_slash(&target);
        if added.is_empty() && kept.is_empty() {
            writes.notes.push(format!("the project's settings at {where_to} already say all of it"));
        }
        if !added.is_empty() {
            writes.notes.push(format!("added to {where_to}: {}", added.join(", ")));
        }
        if !kept.is_empty() {
            writes.notes.push(format!(
                "kept the project's own values for: {} (the template's differ)",
                kept.join(", ")
            ));
        }
        writes.files.push((target, rendered));
        Ok(())
    }

    /// Copy the files a *project's* validation reads: its manifest, its default
    /// machine and bundle, the settings every member resolves through it, and
    /// each member rhei's own plan files.
    ///
    /// Mirroring the member alone is what let a union report success over a
    /// write that made every project-scoped command fail — the same isolation
    /// §FS-rhei-templates.6.2 names for the `--output` path.
    /// §AR-rhei-library.5
    fn mirror_project_files(project: &Path, mirror: &Path) -> MietteResult<()> {
        mirror_plan_files(project, mirror)?;
        let manifest = project.join(workspace::PANTA_INDEX_FILE);
        copy_tree(&manifest, &mirror.join(workspace::PANTA_INDEX_FILE))?;
        if let Some(found) = resolve_rhei_home_file(project, WORKSPACE_SETTINGS_FILE) {
            if let Ok(relative) = found.path().strip_prefix(project) {
                copy_tree(found.path(), &mirror.join(relative))?;
            }
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(project)
            .map_err(|err| file_io_report(project, "failed to read the project", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join("index.rhei.md").is_file())
            .collect();
        entries.sort();
        for entry in entries {
            let Some(name) = entry.file_name() else { continue };
            mirror_plan_files(&entry, &mirror.join(name))?;
        }
        Ok(())
    }
