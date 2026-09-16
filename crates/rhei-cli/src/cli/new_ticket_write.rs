// Where a new ticket's markdown lands: which file, and how it is spliced in.
//
// Its own part because placement is a layout question — single file, workspace,
// basin — with no knowledge of ids, kinds, or state machines.

// §FS-rhei-new.3.1

/// A rhei's on-disk shape, which is what decides where a ticket goes.
enum RheiEntry {
    /// A single-file rhei: one `.rhei.md` holding every ticket.
    SingleFile(PathBuf),
    /// A Directory Workspace rhei: the directory holding `tasks/`.
    Workspace(PathBuf),
    /// The project basin: a directory of task files with no authored index,
    /// created on demand. §FS-rhei-panta.2 §AR-rhei-panta.1
    Basin(PathBuf),
}

/// The decided write for one ticket.
struct PlacedTicket {
    path: PathBuf,
    contents: String,
    dirs: Vec<PathBuf>,
}

/// The rendered ticket data that determines its destination and contents.
struct RenderedTicket<'a> {
    local_id: &'a str,
    title: &'a str,
    block: &'a str,
}

/// Locate the rhei that owns a ticket, in whichever layout it uses.
fn resolve_rhei_entry(
    target: &Path,
    loaded: &LoadedPlan,
    rhei_id: &str,
) -> MietteResult<RheiEntry> {
    if let Some(project_dir) = workspace::panta_project_dir(target) {
        if rhei_id == workspace::BASIN_RHEI_ID {
            return Ok(RheiEntry::Basin(project_dir.join(workspace::BASIN_RHEI_ID)));
        }
        let entries = workspace::discover_rhei_entries(&project_dir)
            .map_err(|err| nested_parse_report(&err))?;
        for entry in entries {
            let matches = if entry.is_dir() {
                entry.file_name().and_then(|name| name.to_str()) == Some(rhei_id)
            } else {
                entry
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_suffix(".rhei.md"))
                    == Some(rhei_id)
            };
            if matches {
                return Ok(if entry.is_dir() {
                    RheiEntry::Workspace(entry)
                } else {
                    RheiEntry::SingleFile(entry)
                });
            }
        }
        return Err(miette!(
            help = did_you_mean(rhei_id, &loaded.rhei_ids)
                .unwrap_or_else(|| "add it first: `rhei new \"<title>\"`.".to_string()),
            "no rhei '{rhei_id}' in the project at {}",
            display_path(&project_dir)
        ));
    }

    // Outside a project the plan itself is the one rhei. §AR-rhei-panta.2
    if rhei_id == workspace::BASIN_RHEI_ID {
        return Err(miette!(
help = "run `rhei init` to make this a project, then capture with `--under basin`.",

            "the basin exists only inside a Panta project; {} is a lone plan",
            display_path(target)
        ));
    }
    match workspace::workspace_dir(target) {
        Some(dir) => Ok(RheiEntry::Workspace(dir)),
        None => Ok(RheiEntry::SingleFile(target.to_path_buf())),
    }
}

/// The limits a new ticket is checked against, and whether the rhei authored
/// them at all.
///
/// The second half is what a refusal has to know: a rhei created without
/// `--max-levels` carries no frontmatter block, and telling its author to raise
/// a field that is not in the file is advice they cannot follow.
// §FS-rhei-new.3.3
struct RheiStructure {
    structure: rhei_core::ast::Structure,
    /// True when the source file carries a `structure:` frontmatter block.
    declared: bool,
}

/// True when `raw` authors a `structure:` block, rather than inheriting the
/// defaults. The block is frontmatter, so the key is flush-left on its own
/// line. §FS-rhei-plan-language.1.1
fn declares_structure(raw: &str) -> bool {
    raw.lines().any(|line| line.trim_end() == "structure:")
}

/// The structure a rhei declares — the limits a new ticket is checked against.
/// The basin has no authored index, so it takes the project manifest's.
// §AR-rhei-panta.1 §FS-rhei-new.3.3
fn rhei_entry_structure(entry: &RheiEntry, target: &Path) -> MietteResult<RheiStructure> {
    match entry {
        RheiEntry::SingleFile(path) => {
            let raw = read_input_file(path)?;
            let rhei = rhei_core::parse(&raw).map_err(|err| parse_report(path, &raw, &err))?;
            Ok(RheiStructure { structure: rhei.structure, declared: declares_structure(&raw) })
        }
        RheiEntry::Workspace(dir) => {
            let path = dir.join("index.rhei.md");
            let raw = read_input_file(&path)?;
            let index = rhei_core::parser::parse_workspace_index(&raw)
                .map_err(|err| parse_report(&path, &raw, &err))?;
            Ok(RheiStructure { structure: index.structure, declared: declares_structure(&raw) })
        }
        RheiEntry::Basin(_) => {
            let Some(project_dir) = workspace::panta_project_dir(target) else {
                return Ok(RheiStructure {
                    structure: rhei_core::ast::Structure::default(),
                    declared: false,
                });
            };
            let path = project_dir.join(workspace::PANTA_INDEX_FILE);
            let raw = read_input_file(&path)?;
            let manifest = rhei_core::parser::parse_panta_manifest(&raw)
                .map_err(|err| parse_report(&path, &raw, &err))?;
            Ok(RheiStructure { structure: manifest.structure, declared: declares_structure(&raw) })
        }
    }
}

/// Decide the file and its new contents. A top-level ticket appends (single
/// file) or becomes a new task file (workspace, basin); a subtask always goes
/// into the file that already holds its parent. §FS-rhei-new.3.1
fn place_ticket(
    entry: &RheiEntry,
    placement: &TicketParent,
    loaded: &LoadedPlan,
    target: &Path,
    ticket: &RenderedTicket<'_>,
    decision: NewDecision,
) -> MietteResult<PlacedTicket> {
    if let Some(parent_local) = &placement.parent_local {
        let qualified_parent = format!("{}.{}", placement.rhei_id, parent_local);
        let path = match entry {
            RheiEntry::SingleFile(path) => path.clone(),
            // A task file owns a subtree: splitting one across files would put
            // a parent and its child in different diffs.
            _ => loaded.task_file(&qualified_parent, target),
        };
        let raw = read_input_file(&path)?;
        let contents =
            insert_ticket_after_subtree(&raw, parent_local, ticket.block).ok_or_else(|| {
            miette!(
help = "re-run after `rhei validate` passes, so the plan on disk and the ids agree.",

                "could not find the heading for ticket {qualified_parent} in {}",
                display_path(&path)
            )
        })?;
        return Ok(PlacedTicket { path, contents, dirs: Vec::new() });
    }

    match entry {
        RheiEntry::SingleFile(path) => {
            let raw = read_input_file(path)?;
            Ok(PlacedTicket {
                path: path.clone(),
                contents: append_ticket(&raw, ticket.block),
                dirs: Vec::new(),
            })
        }
        RheiEntry::Workspace(dir) => {
            let tasks_dir = dir.join("tasks");
            let path = tasks_dir.join(task_file_name(ticket.local_id, ticket.title));
            // Absence becomes authoritative only after this path's sidecar is
            // held by the repeated decision. §FS-rhei-new.4
            if decision == NewDecision::Authoritative {
                reject_existing_destination(&path)?;
            }
            Ok(PlacedTicket {
                path,
                contents: ticket.block.to_string(),
                dirs: vec![tasks_dir],
            })
        }
        RheiEntry::Basin(dir) => {
            let path = dir.join(task_file_name(ticket.local_id, ticket.title));
            // Same protocol as a workspace task file. §FS-rhei-new.4
            if decision == NewDecision::Authoritative {
                reject_existing_destination(&path)?;
            }
            Ok(PlacedTicket { path, contents: ticket.block.to_string(), dirs: vec![dir.clone()] })
        }
    }
}

/// Refuse a destination that is already occupied.
///
/// A task file's name comes from the id and the title, so two unrelated
/// tickets can pick the same one — and the file already there is authored
/// content, not a slot. Creating is not editing: an unconditional write would
/// take someone's notes, exit 0, and validate green.
// §FS-rhei-new.5.1 §FS-rhei-new.6
fn reject_existing_destination(path: &Path) -> MietteResult<()> {
    if !path.exists() {
        return Ok(());
    }
    Err(miette!(
help = "give the ticket another id with --id, or move the existing file aside first.",

        "{} already exists, and `rhei new` never writes over a file it did not create",
        display_path(path)
    ))
}

/// `004-rotate-signing-keys.md` — the id first so the directory sorts the way
/// the plan reads, the slug so a human can find the file. §FS-rhei-new.3.1
fn task_file_name(local_id: &str, title: &str) -> String {
    let stem = padded_task_file_stem(local_id);
    match derive_rhei_id(title) {
        Some(slug) => format!("{stem}-{slug}.md"),
        None => format!("{stem}.md"),
    }
}

/// Zero-pad a numeric ticket id to three digits, the width every shipped
/// template already writes (`01-coordinate.md` is the two-digit ancestor).
///
/// Path order *is* plan order, and commands that scan in plan order schedule
/// in it: unpadded, `10-…` sorts between `1-…` and
/// `2-…`, so the eleventh ticket in a rhei is picked up second and nothing
/// reports a problem — `rhei validate` succeeds, because the file is fine and
/// only its name is out of order. Named ids keep their name: a name has no
/// numeric order to preserve.
// §FS-rhei-new.3.1 §FS-rhei-plan-language.1.2
fn padded_task_file_stem(local_id: &str) -> String {
    match local_id.parse::<u32>() {
        Ok(number) => format!("{number:03}"),
        Err(_) => local_id.to_string(),
    }
}
