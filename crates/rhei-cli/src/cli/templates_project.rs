    /// Where an instantiated template lands relative to a Panta project. A
    /// member rhei keeps whatever machine it declares — the project default
    /// only covers rheis that declare nothing. §FS-rhei-templates.6.2
    pub(super) enum ProjectPlacement {
        /// The output is not a member of any project; nothing to reconcile.
        Standalone,
        /// The output joins `project` as a member rhei.
        Member { project: PathBuf },
    }

    impl ProjectPlacement {
        pub(super) fn project(&self) -> Option<&Path> {
            match self {
                ProjectPlacement::Standalone => None,
                ProjectPlacement::Member { project } => Some(project.as_path()),
            }
        }
    }

    /// Say what joining a project did to it. The settings commit is a write
    /// outside the member directory, so it may not happen silently.
    /// §FS-rhei-templates.6.2
    fn report_project_placement(
        placement: &ProjectPlacement,
        prepared: Option<&PreparedProjectSettings>,
    ) {
        let Some(project) = placement.project() else {
            return;
        };
        println!("Added to the Panta project at {}.", display_path(project).display());
        if let Some(prepared) = prepared {
            println!(
                "  Merged the template's agent settings into {}.",
                display_path(&project_settings_write_path(project)).display()
            );
            if !prepared.added.is_empty() {
                println!("    added: {}", prepared.added.join(", "));
            }
            if !prepared.kept.is_empty() {
                println!(
                    "    kept your existing values for: {} (the template's differ)",
                    prepared.kept.join(", ")
                );
            }
        }
    }

    /// The project a *new* rhei created from the current directory belongs to,
    /// following the same walk `rhei` uses to resolve an omitted target:
    /// the directory itself, then its `panta/` child. §FS-rhei-panta.6
    pub(super) fn enclosing_project_for_new_rhei(start: &Path) -> Option<PathBuf> {
        let mut current = Some(start);
        while let Some(dir) = current {
            if workspace::is_panta_project(dir) {
                return Some(dir.to_path_buf());
            }
            let nested = dir.join("panta");
            if workspace::is_panta_project(&nested) {
                return Some(nested);
            }
            current = dir.parent();
        }
        None
    }

    /// The project that would *discover* `output_dir`, which is only ever its
    /// immediate parent: discovery reads the entries sitting directly next to
    /// `index.panta.md` and never recurses. §AR-rhei-panta.1
    fn owning_project_of(output_dir: &Path) -> Option<PathBuf> {
        let parent = output_dir.parent()?;
        workspace::is_panta_project(parent).then(|| parent.to_path_buf())
    }

    /// Decide how `output_dir` relates to any enclosing project. State
    /// machines never conflict — a member rhei's own declaration simply
    /// overrides the project default — so the only placement questions left
    /// are about discovery.
    // §FS-rhei-templates.6.2
    pub(super) fn plan_project_placement(
        output_dir: &Path,
        materialized_dir: &Path,
    ) -> MietteResult<ProjectPlacement> {
        let Some(project) = owning_project_of(output_dir) else {
            // Not a member — but a workspace buried deeper inside a project is
            // invisible to it, and silently producing something the project
            // will never list is the failure this whole check exists to stop.
            if let Some(outer) = enclosing_project_for_new_rhei(output_dir) {
                if output_dir.starts_with(&outer) {
                    eprintln!(
                        "warning: {} is inside the Panta project at {}, but not directly next to \
                         index.panta.md, so the project will not discover it. Instantiate into \
                         {} to make it a member.",
                        display_path(output_dir).display(),
                        display_path(&outer).display(),
                        display_path(&outer.join(output_dir.file_name().unwrap_or_default()))
                            .display()
                    );
                }
            }
            return Ok(ProjectPlacement::Standalone);
        };

        // Discovery counts `*.rhei.md` files and Directory Workspaces, nothing
        // else. A single-file template renders into a plain directory holding
        // `plan.rhei.md`, which is neither. §AR-rhei-panta.1
        if !workspace::is_workspace(materialized_dir) {
            eprintln!(
                "warning: {} sits in the Panta project at {}, but a single-file template renders                  a plain directory, which discovery does not count as a rhei — only `*.rhei.md`                  files and Directory Workspaces. The project will not list it. Instantiate it                  outside the project (`--output {}`), or move the rendered plan file next to                  index.panta.md.",
                display_path(output_dir).display(),
                display_path(&project).display(),
                display_path(&sibling_output_suggestion(&project, output_dir)).display()
            );
            return Ok(ProjectPlacement::Standalone);
        }

        Ok(ProjectPlacement::Member { project })
    }

    /// A path outside the project, next to it, keeping the chosen directory name.
    fn sibling_output_suggestion(project: &Path, output_dir: &Path) -> PathBuf {
        let name = output_dir.file_name().unwrap_or_default();
        match project.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
            _ => PathBuf::from(name),
        }
    }

    /// The settings file's name under either of rhei's homes.
    /// §FS-rhei-templates.1.1
    const WORKSPACE_SETTINGS_FILE: &str = "settings.json";

    /// What happened to a template's shipped agent settings when its workspace
    /// joined a project.
    pub(super) struct PreparedProjectSettings {
        pub(super) added: Vec<String>,
        pub(super) kept: Vec<String>,
        /// The project file the merge will be written to, what stands there
        /// now, and the hidden source carrying the reconciled validation copy.
        target: PathBuf,
        previous: Option<String>,
        project: PathBuf,
        workspace: PathBuf,
        source: PathBuf,
        rendered: String,
        superseded: Option<PathBuf>,
    }

    impl PreparedProjectSettings {
        /// Commit the reconciled document only after prospective validation.
        /// The hidden workspace copy is removed before member publication.
        // §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.2
        pub(super) fn commit(&self) -> MietteResult<()> {
            if let Some(parent) = self.target.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    file_io_report(parent, "failed to create the project settings directory", err)
                })?;
            }
            if let Err(err) = fs::write(&self.target, &self.rendered) {
                self.undo();
                return Err(file_io_report(
                    &self.target,
                    "failed to write project settings",
                    err,
                ));
            }
            if let Err(err) = fs::remove_file(&self.source) {
                self.undo();
                return Err(file_io_report(
                    &self.source,
                    "failed to remove staged workspace settings",
                    err,
                ));
            }
            prune_empty_parents(&self.source, &self.workspace);
            if let Some(superseded) = &self.superseded {
                warn_settings_superseded_by_hoist(superseded, &self.target);
            }
            Ok(())
        }

        /// Put the project's settings back the way a discarded instantiation
        /// found them: the merge removed along with the directories written
        /// for it, or the pre-merge content restored. Instantiation validates
        /// *after* hoisting, and a command that failed must not change which
        /// settings file the project reads — least of all onto a file holding
        /// a template it never instantiated. §FS-rhei-templates.6.2
        pub(super) fn undo(&self) {
            match &self.previous {
                Some(content) => {
                    let _ = fs::write(&self.target, content);
                }
                None => {
                    if fs::remove_file(&self.target).is_ok() {
                        prune_empty_parents(&self.target, &self.project);
                    }
                }
            }
        }
    }

    /// Prepare the project settings merge in hidden staging. Existing project
    /// values win, so a template never redefines a configured agent.
    /// §FS-rhei-agents.1.1 §FS-rhei-templates.6.2
    ///
    /// Both ends resolve `.agent-grounds/rhei/` before the deprecated
    /// `.agents/rhei/`, and the merged result is written to the current home
    /// whichever one it was read from: rhei never writes the deprecated path.
    /// §FS-rhei-templates.1.1
    pub(super) fn prepare_workspace_settings_for_project(
        workspace: &Path,
        project: &Path,
    ) -> MietteResult<Option<PreparedProjectSettings>> {
        let Some(source) = resolve_rhei_home_file(workspace, WORKSPACE_SETTINGS_FILE) else {
            return Ok(None);
        };
        source.warn_if_deprecated();
        let incoming = read_settings_value(source.path())?;

        // The merge base is whatever the project already resolves, so a hoist
        // into a project still on the old home carries its values forward
        // rather than shadowing them. §FS-rhei-agents.1.1

        // Reading it from the deprecated home is reported once the merge
        // exists, by the message below. §FS-rhei-templates.6.2
        let existing = resolve_rhei_home_file(project, WORKSPACE_SETTINGS_FILE);
        let mut merged = match &existing {
            Some(found) => read_settings_value(found.path())?,
            None => serde_json::json!({}),
        };
        let target = project_settings_write_path(project);

        let mut added = Vec::new();
        let mut kept = Vec::new();
        merge_settings_value(&mut merged, &incoming, "", &mut added, &mut kept);

        let rendered = serde_json::to_string_pretty(&merged)
            .map_err(|err| miette!(
help = template_manifest_help(),
"failed to render merged settings: {err}"))?;
        let rendered = format!("{rendered}\n");
        // Keep the project untouched for prospective validation. The ordinary
        // settings loader reads this reconciled document from staging instead.
        // §FS-rhei-templates.6.1.2
        let previous = fs::read_to_string(&target).ok();
        fs::write(source.path(), &rendered).map_err(|err| {
            file_io_report(source.path(), "failed to stage reconciled project settings", err)
        })?;

        Ok(Some(PreparedProjectSettings {
            added,
            kept,
            target,
            previous,
            project: project.to_path_buf(),
            workspace: workspace.to_path_buf(),
            source: source.path().to_path_buf(),
            rendered,
            superseded: existing
                .as_ref()
                .and_then(RheiHomePath::deprecated_path)
                .map(Path::to_path_buf),
        }))
    }

    /// What became of a project settings file the hoist read from the deprecated
    /// home. The generic move-warning cannot be the last word here: it names the
    /// path the hoist has just written itself, so obeying it would drop the
    /// template's entries. Claim that path's one warning too, so the generic
    /// message cannot follow later in the process and contradict this one.
    /// §FS-rhei-templates.6.2
    fn warn_settings_superseded_by_hoist(superseded: &Path, merged: &Path) {
        let _ = claim_deprecated_rhei_home_warning(superseded);
        eprintln!(
            "warning: {} is deprecated, and the merge just written to {} supersedes it — \
             the merged file holds that file's keys plus the template's. Nothing reads the \
             deprecated file any more, and moving it over the merged one would drop the \
             template's entries, so delete it once the merge looks right.",
            superseded.display(),
            merged.display()
        );
    }

    /// Where a project's settings file is written. §FS-rhei-templates.1.1
    pub(super) fn project_settings_write_path(project: &Path) -> PathBuf {
        rhei_home_write_path(project, WORKSPACE_SETTINGS_FILE)
    }

    fn read_settings_value(path: &Path) -> MietteResult<serde_json::Value> {
        let content = fs::read_to_string(path)
            .map_err(|err| file_io_report(path, "failed to read settings", err))?;
        serde_json::from_str(&content)
            .map_err(|err| miette!(
help = settings_help(),
"failed to parse {}: {err}", path.display()))
    }

    /// Deep-merge `incoming` into `target`, recording which leaf keys were added
    /// and which the target already defined differently.
    fn merge_settings_value(
        target: &mut serde_json::Value,
        incoming: &serde_json::Value,
        path: &str,
        added: &mut Vec<String>,
        kept: &mut Vec<String>,
    ) {
        let (Some(target_map), Some(incoming_map)) = (target.as_object_mut(), incoming.as_object())
        else {
            return;
        };
        for (key, value) in incoming_map {
            let child_path =
                if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
            match target_map.get_mut(key) {
                None => {
                    target_map.insert(key.clone(), value.clone());
                    added.push(child_path);
                }
                Some(existing) if existing.is_object() && value.is_object() => {
                    merge_settings_value(existing, value, &child_path, added, kept);
                }
                Some(existing) if existing == value => {}
                Some(_) => kept.push(child_path),
            }
        }
    }

    /// Remove directories left empty by the hoist, stopping at `stop_at`.
    fn prune_empty_parents(removed: &Path, stop_at: &Path) {
        let mut current = removed.parent();
        while let Some(dir) = current {
            if dir == stop_at || !dir.starts_with(stop_at) {
                return;
            }
            if fs::read_dir(dir).map(|mut entries| entries.next().is_some()).unwrap_or(true) {
                return;
            }
            if fs::remove_dir(dir).is_err() {
                return;
            }
            current = dir.parent();
        }
    }
