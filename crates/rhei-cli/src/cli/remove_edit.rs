// The files `rhei remove` rewrites, decided in memory as whole before and after
// images: the retirement record first, then the definition and its metadata.
//
// Its own part because the same images are what the pending-removal marker
// records and what a resumed removal is allowed to find.

// §FS-rhei-remove.4.1 §FS-rhei-remove.5.1 §FS-rhei-remove.6.2

/// One file's whole contents before and after removal; `None` before is a file
/// that does not exist yet.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemovalImage {
    path: PathBuf,
    before: Option<String>,
    after: String,
}

/// Every image, in the order they are installed. §FS-rhei-remove.6.2
#[derive(Debug, Default)]
struct RemovalImages(Vec<RemovalImage>);

impl RemovalImages {
    /// The current after-image of `path`, read from disk the first time.
    fn current(&mut self, path: &Path) -> MietteResult<&mut RemovalImage> {
        if let Some(index) = self.0.iter().position(|image| image.path == path) {
            return Ok(&mut self.0[index]);
        }
        let before = match fs::read_to_string(path) {
            Ok(raw) => Some(raw),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => return Err(file_io_report(path, "failed to read", err)),
        };
        let after = before.clone().unwrap_or_default();
        self.0.push(RemovalImage { path: path.to_path_buf(), before, after });
        Ok(self.0.last_mut().expect("just pushed"))
    }
}

/// The document holding the project's retirement record: the manifest of a
/// Panta project, the lone rhei's own metadata document otherwise.
/// §FS-rhei-remove.5.1
fn retirement_document(loaded: &LoadedPlan, input: &Path, id: &str) -> PathBuf {
    if loaded.is_panta_project() {
        let project = workspace::panta_project_dir(input).unwrap_or_else(|| input.to_path_buf());
        return project.join(workspace::PANTA_INDEX_FILE);
    }
    loaded.task_route(id, input).metadata_file
}

/// Decide every rewrite in memory. §FS-rhei-remove.4.1 §FS-rhei-remove.5.1
fn plan_removal_images(loaded: &LoadedPlan, input: &Path, id: &str) -> MietteResult<RemovalImages> {
    let mut images = RemovalImages::default();
    let route = loaded.task_route(id, input);

    // The retirement is persisted first, so an interruption can never leave a
    // freed number behind. §FS-rhei-remove.6.2
    let document = retirement_document(loaded, input, id);
    let record = rhei_core::retired::RetiredTicket { budget_ticket_id: budget_ticket_id(loaded, id) };
    let image = images.current(&document)?;
    let raw = image.after.clone();
    let mut metadata = parse_document_metadata(&document, &raw)?.unwrap_or_default();
    rhei_core::retired::record_retirement(&mut metadata, id, &record).map_err(|err| {
        miette!(help = "the key is reserved for the retirement record; rename yours.", "{err}")
    })?;
    image.after = rewrite_frontmatter(&raw, &metadata)?;

    let image = images.current(&route.metadata_file)?;
    let raw = image.after.clone();
    if let Some(mut metadata) = parse_document_metadata(&route.metadata_file, &raw)? {
        if remove_task_metadata_entry(&mut metadata, &route.metadata_id) {
            image.after = rewrite_frontmatter(&raw, &metadata)?;
        }
    }

    let image = images.current(&route.task_file)?;
    let mut after = remove_ticket_section(&image.after, &route.local_id).ok_or_else(|| {
        miette!(
            help = "the plan changed under removal; re-run it.",
            "could not find the section of {id} in {}",
            display_path(&route.task_file)
        )
    })?;
    // A workspace task file's own metadata block names only its own tasks, so
    // the entry goes with the task. §FS-rhei-remove.4.1
    if route.task_file != route.metadata_file {
        after = remove_task_file_entry(&after, &route.local_id)?;
    }
    image.after = after;
    images.0.retain(|image| image.before.as_deref() != Some(image.after.as_str()));
    Ok(images)
}

fn parse_document_metadata(path: &Path, raw: &str) -> MietteResult<Option<Metadata>> {
    rhei_core::metadata::parse_metadata_file(path, raw).map_err(|err| parse_report(path, raw, &err.error))
}

fn yaml_key_text(key: &YamlValue) -> Option<String> {
    match key {
        YamlValue::String(text) => Some(text.clone()),
        YamlValue::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Drop `metadata.tasks.<key>` and the containers it leaves empty. Returns
/// whether there was an entry to drop. §FS-rhei-remove.4.1
fn remove_task_metadata_entry(metadata: &mut Metadata, key: &str) -> bool {
    let Some(YamlValue::Mapping(section)) = metadata.get_mut("metadata") else { return false };
    let Some(YamlValue::Mapping(tasks)) = section.get_mut("tasks") else { return false };
    let before = tasks.len();
    tasks.retain(|entry, _| yaml_key_text(entry).as_deref() != Some(key));
    let removed = tasks.len() != before;
    if tasks.is_empty() {
        section.remove("tasks");
    }
    if section.is_empty() {
        metadata.remove("metadata");
    }
    removed
}

/// Delete exactly one ticket's section: its heading through the line before the
/// next sibling or ancestor heading. `None` when the heading is not there.
/// §FS-rhei-remove.4.1
fn remove_ticket_section(raw: &str, local_id: &str) -> Option<String> {
    let lines: Vec<&str> = raw.split_inclusive('\n').collect();
    let mut in_code = false;
    let mut start = None;
    let mut level = 0;
    let mut end = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let text = line.trim_end_matches(['\n', '\r']);
        let heading = node_heading_outside_code(text, &mut in_code);
        match start {
            None => {
                if let Some((hashes, id)) = heading {
                    if id == local_id {
                        start = Some(index);
                        level = hashes;
                    }
                }
            }
            Some(_) => {
                let closes = heading.is_some_and(|(hashes, _)| hashes <= level)
                    || (!in_code && (text.starts_with("# ") || text.starts_with("## ")));
                if closes {
                    end = index;
                    break;
                }
            }
        }
    }
    let start = start?;
    let mut kept: String = lines[..start].concat();
    if end == lines.len() {
        while kept.ends_with("\n\n") {
            kept.pop();
        }
    }
    kept.push_str(&lines[end..].concat());
    Some(kept)
}

/// Remove the ticket's entry from a workspace task file's own metadata block,
/// dropping the block when nothing is left in it. §FS-rhei-remove.4.1
fn remove_task_file_entry(raw: &str, local_id: &str) -> MietteResult<String> {
    let Some(rest) = raw.strip_prefix("---\n") else { return Ok(raw.to_string()) };
    let Some(close) = rest.find("\n---\n").map(|at| at + 1).or_else(|| rest.starts_with("---\n").then_some(0))
    else {
        return Ok(raw.to_string());
    };
    let (yaml, tail) = (&rest[..close], &rest[close + "---\n".len()..]);
    let Ok(mut metadata) = serde_yaml::from_str::<Metadata>(yaml) else { return Ok(raw.to_string()) };
    if !remove_task_metadata_entry(&mut metadata, local_id) {
        return Ok(raw.to_string());
    }
    if metadata.is_empty() {
        return Ok(tail.trim_start_matches('\n').to_string());
    }
    let rendered = serde_yaml::to_string(&metadata)
        .map_err(|err| miette!(help = "report this as a rhei bug.", "failed to render metadata: {err}"))?;
    Ok(format!("---\n{rendered}---\n{tail}"))
}
