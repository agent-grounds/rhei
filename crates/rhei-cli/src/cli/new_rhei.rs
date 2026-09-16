// `rhei new <title>` with no `--under`: a new rhei under Panta. §FS-rhei-new.2

/// Decide the file (or workspace) a new rhei becomes. Nothing is written here.
fn new_rhei_write(
    target: &Path,
    options: &NewOptions,
    description: Option<&str>,
    decision: NewDecision,
) -> MietteResult<NewWrite> {
    // A rhei is a *member* of a project; a lone plan has nowhere to put a
    // second one. §FS-rhei-new.2.1
    let Some(project_dir) = workspace::panta_project_dir(target) else {
        return Err(miette!(
help = "create a project first: `rhei init` writes index.panta.md, and `rhei new` fills it.",

            "{} is not a Panta project, so there is nowhere to add a rhei: a project is a \
             directory holding {}, and its rheis live beside that manifest",
            display_path(target),
            workspace::PANTA_INDEX_FILE
        ));
    };
    let id = resolve_new_rhei_id(&options.title, options.id.as_deref())?;
    // The provisional pass selects the pathname; only the repeated pass under
    // its sidecar can authorize absence or adoption. §FS-rhei-new.4
    if decision == NewDecision::Authoritative {
        admit_new_rhei_destination(&project_dir, &id, options.dir)?;
    }

    let header = RheiHeader {
        title: &options.title,
        states: options.states.as_deref(),
        max_levels: options.max_levels,
        node_kinds: &options.node_kinds,
        description,
    };

    let (path, contents, dirs) = if options.dir {
        let rhei_dir = project_dir.join(&id);
        let tasks_dir = rhei_dir.join("tasks");
        // The workspace index carries no `## Tasks`: its tickets live in
        // `tasks/` files. §FS-rhei-plan-language.1.2
        let contents = render_rhei_file(&header, false);
        (rhei_dir.join("index.rhei.md"), contents, vec![rhei_dir, tasks_dir])
    } else {
        let contents = render_rhei_file(&header, true);
        (project_dir.join(format!("{id}.rhei.md")), contents, Vec::new())
    };

    Ok(NewWrite {
        kind: "rhei",
        id: id.clone(),
        title: options.title.trim().to_string(),
        path,
        state: None,
        preview: contents.clone(),
        contents,
        dirs,
        next_hint: Some(format!("`rhei new \"<first ticket>\" --under {id}`")),
        notes: Vec::new(),
    })
}

/// The filesystem shapes a rhei create can meet after its layout is selected.
/// §FS-rhei-new.2.1.1 §FS-rhei-new.4
#[derive(Debug, Eq, PartialEq)]
enum NewRheiDestination {
    Vacant,
    AdoptableDirectory,
    ExistingRhei(PathBuf),
    LayoutConflict { directory: PathBuf, obstruction: Option<PathBuf> },
    Occupied(PathBuf),
}

/// Classify the two same-id paths without trying to load an authored machine.
/// Normal discovery takes over only after the workspace index has been written.
/// §FS-rhei-new.2.1.1 §FS-rhei-new.4
fn classify_new_rhei_destination(
    project_dir: &Path,
    id: &str,
    directory_layout: bool,
) -> MietteResult<NewRheiDestination> {
    let file = project_dir.join(format!("{id}.rhei.md"));
    let dir = project_dir.join(id);
    let file_kind = destination_file_type(&file)?;
    let dir_kind = destination_file_type(&dir)?;

    if file_kind.as_ref().is_some_and(std::fs::FileType::is_file) {
        return Ok(NewRheiDestination::ExistingRhei(file));
    }
    if dir_kind.as_ref().is_some_and(std::fs::FileType::is_dir)
        && dir.join(workspace::RHEI_INDEX_FILE).is_file()
    {
        return Ok(NewRheiDestination::ExistingRhei(dir));
    }

    if directory_layout {
        return match dir_kind {
            None => Ok(NewRheiDestination::Vacant),
            Some(kind) if kind.is_dir() => match prospective_workspace_obstruction(&dir)? {
                Some(path) => Ok(NewRheiDestination::Occupied(path)),
                None => Ok(NewRheiDestination::AdoptableDirectory),
            },
            Some(_) => Ok(NewRheiDestination::Occupied(dir)),
        };
    }

    if dir_kind.as_ref().is_some_and(std::fs::FileType::is_dir) {
        return Ok(NewRheiDestination::LayoutConflict {
            obstruction: prospective_workspace_obstruction(&dir)?,
            directory: dir,
        });
    }
    match file_kind {
        None => Ok(NewRheiDestination::Vacant),
        Some(_) => Ok(NewRheiDestination::Occupied(file)),
    }
}

/// Inspect one destination path without following a symlink into content the
/// create does not own. §FS-rhei-new.2.1.1 §FS-rhei-new.5.1
fn destination_file_type(path: &Path) -> MietteResult<Option<std::fs::FileType>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata.file_type())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(file_io_report(path, "failed to inspect destination", err)),
    }
}

/// Return the first entry that keeps a real directory from being an admissible
/// empty-or-machine-bundle workspace. Machine syntax is deliberately unread.
/// §FS-rhei-new.2.1.1
fn prospective_workspace_obstruction(dir: &Path) -> MietteResult<Option<PathBuf>> {
    let mut entries = fs::read_dir(dir)
        .map_err(|err| file_io_report(dir, "failed to read prospective workspace", err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| file_io_report(dir, "failed to read prospective workspace", err))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    let mut has_states = false;
    let mut prompt_templates = None;
    for entry in entries {
        let path = entry.path();
        let file_type =
            entry.file_type().map_err(|err| file_io_report(&path, "failed to inspect", err))?;
        match entry.file_name().to_str() {
            Some("states.yaml") if file_type.is_file() => has_states = true,
            Some("prompt_templates") if file_type.is_dir() => prompt_templates = Some(path),
            Some("index.rhei.md.lock") if file_type.is_file() => {
                let metadata = entry
                    .metadata()
                    .map_err(|err| file_io_report(&path, "failed to inspect", err))?;
                if metadata.len() != 0 {
                    return Ok(Some(path));
                }
                // The exact empty regular destination sidecar is permanent
                // coordination state, not authored workspace content.
                // §FS-rhei-new.2.1.1
            }
            _ => return Ok(Some(path)),
        }
    }
    if !has_states {
        return Ok(prompt_templates);
    }
    Ok(None)
}

/// Enforce the selected layout's destination decision before any write.
/// §FS-rhei-new.2.1.1 §FS-rhei-new.4
fn admit_new_rhei_destination(
    project_dir: &Path,
    id: &str,
    directory_layout: bool,
) -> MietteResult<()> {
    match classify_new_rhei_destination(project_dir, id, directory_layout)? {
        NewRheiDestination::Vacant | NewRheiDestination::AdoptableDirectory => Ok(()),
        NewRheiDestination::ExistingRhei(path) => Err(miette!(
help = "pick another id with --id, or add a ticket to the existing rhei with `--under`.",

            "rhei '{id}' already exists at {}",
            display_path(&path)
        )),
        NewRheiDestination::LayoutConflict { directory, obstruction: None } => Err(miette!(
help = "re-run with `--dir` to adopt that empty or authored-machine workspace.",

            "cannot create single-file rhei '{id}': {} is a same-id directory, so the requested layout conflicts with it",
            display_path(&directory)
        )),
        NewRheiDestination::LayoutConflict { directory, obstruction: Some(path) } => Err(miette!(
help = format!(
    "move or remove {}, then re-run; use `--dir` only when the directory is empty or contains an authored machine bundle.",
    display_path(&path)
),

            "cannot create single-file rhei '{id}': the same-id directory {} is occupied by {}",
            display_path(&directory),
            display_path(&path)
        )),
        NewRheiDestination::Occupied(path) => Err(miette!(
help = format!("move or remove {}, then re-run.", display_path(&path)),

            "cannot create rhei '{id}': the destination is occupied by {}",
            display_path(&path)
        )),
    }
}
