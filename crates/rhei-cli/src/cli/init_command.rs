// `rhei init` — set up a Panta project: a gitignored `panta/` folder by
// default, or the host itself with `--here` (adoption). §FS-rhei-init

const AGENTS_NOTE_BEGIN: &str = "<!-- rhei:begin -->";
const AGENTS_NOTE_END: &str = "<!-- rhei:end -->";

/// Note body shared by both modes after the location sentence. §FS-rhei-init.4
const AGENTS_NOTE_TAIL: &str = "Plans are
`*.rhei.md` files and workspace directories; ticket ids are
project-qualified (`<rhei>.<id>`). Add work with
`rhei new \"<title>\" --under <rhei>`, and capture a ticket that has no
rhei yet with `--under basin`. Work tickets with `rhei list`,
`rhei next`, and `rhei complete`; validate edits with `rhei validate`.
Orchestration (`rhei run`) is started by humans, never by agents.";

fn init_command(
    dir: Option<&Path>,
    title: Option<&str>,
    no_agents: bool,
    force: bool,
    here: bool,
) -> MietteResult<()> {
    let host = match dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|err| miette!(
help = cwd_help(),
"failed to read the current directory: {err}"))?,
    };
    let project = if here { host.clone() } else { host.join("panta") };
    // Reinitialization writes the basin's shared manifest too. §FS-rhei-recover.4
    let _root_guards = if project.is_dir() {
        rhei_core::root_access::for_input(&project).map_err(|err| miette!("{err}"))?
    } else { Vec::new() };


    // §FS-rhei-init.2: a host that is itself a project refuses default mode
    // even under --force — a fresh `panta/` child nested inside it would lose
    // every target resolution to the host manifest and never be reachable.
    if !here && host.join("index.panta.md").is_file() {
        return Err(miette!(
help = init_conflict_help(),

            "{} is already a Panta project itself: index.panta.md exists at the host. \
             Re-run with `--force --here` to re-initialize it in place; a new `panta/` \
             project inside it would be shadowed by the host manifest",
            host.display()
        ));
    }
    // §FS-rhei-init.2: --here must not shadow an existing `panta/` project —
    // target resolution prefers the host manifest (§FS-rhei-panta.6). Not
    // even --force skips this: force never means "bury the child project".
    let conventional = host.join("panta");
    if here
        && !host.join("index.panta.md").is_file()
        && conventional.join("index.panta.md").is_file()
    {
        return Err(miette!(
help = init_conflict_help(),

            "{} already holds a Panta project at {}: an adopted host project would \
             shadow it — every inferred target would resolve to the host manifest. \
             Keep using the project at {}, or move its contents into the host and \
             remove it before re-running `rhei init --here`",
            host.display(),
            conventional.display(),
            conventional.display()
        ));
    }
    // §FS-rhei-init.2: refuse an existing project untouched unless --force,
    // which rewrites the manifest and updates companion files in place.
    if project.join("index.panta.md").is_file() && !force {
        return Err(miette!(
help = init_conflict_help(),

            "{} is already a Panta project: index.panta.md exists. Re-run with \
             `--force` to overwrite the manifest",
            project.display()
        ));
    }
    // §FS-rhei-init.2: a `panta/` project would not discover plans sitting in
    // the host — refuse rather than silently shadow them.
    if !here && !force {
        let stranded = workspace::discover_rhei_entries(&host).unwrap_or_default();
        if !stranded.is_empty() {
            let names: Vec<String> = stranded
                .iter()
                .filter_map(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .collect();
            return Err(miette!(
help = init_conflict_help(),

                "{} already holds {} rhei(s) ({}) that a `panta/` project would not \
                 discover. Re-run with `--here` to adopt them in place, or move them \
                 into panta/ first",
                host.display(),
                names.len(),
                names.join(", ")
            ));
        }
    }

    fs::create_dir_all(&project)
        .map_err(|err| file_io_report(&project, "failed to create", err))?;

    // §FS-rhei-init.2: nested projects are almost always a mistake.
    if let Some(outer) = enclosing_panta_project(&project) {
        eprintln!(
            "warning: {} is inside the Panta project at {}; the outer project will not \
             discover this one",
            project.display(),
            outer.display()
        );
    }

    // §FS-rhei-init.1: the host names the project in both modes — `panta/`
    // is a location, not an identity.
    let title = match title {
        Some(title) => title.to_string(),
        None => default_project_title(&host),
    };
    // §FS-rhei-init.2: the manifest is bare — each rhei keeps the machine it
    // declares, and ones declaring nothing run the built-in default.
    let contents = format!("# Panta: {title}\n");
    let manifest = project.join("index.panta.md");
    fs::write(&manifest, contents)
        .map_err(|err| file_io_report(&manifest, "failed to write", err))?;

    // §FS-rhei-init.3: default mode ignores the project folder at the host and
    // self-contains the output rules inside it, so un-ignoring the plans later
    // never commits runtime state. Track host writes to name them. §FS-rhei-init.5
    let mut host_changes: Vec<String> = Vec::new();
    if here {
        if seed_gitignore(&host, &["runtime/", ".rhei/cache/"])? {
            host_changes.push(".gitignore".to_string());
        }
    } else {
        if seed_gitignore(&host, &["panta/"])? {
            host_changes.push(".gitignore".to_string());
        }
        seed_gitignore(&project, &["runtime/", ".rhei/cache/"])?;
    }
    if !no_agents {
        let (changed, path) = write_agents_note(&host, here)?;
        if changed {
            // Name the actual file: the note can land in CLAUDE.md when
            // that is the instruction file the host uses. §FS-rhei-init.4
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "AGENTS.md".to_string());
            host_changes.push(name);
        }
    }
    report_initialized_project(&project, &title, here);
    report_host_changes(&host_changes, here);
    if !no_agents {
        report_enclosing_repository_hint(&host);
    }
    println!("Next: `rhei install-skills` wires agent skills; `rhei list` shows the project.");
    // A fresh project's real next step is its first rhei, and the block is
    // long, so it goes last rather than between the setup lines.
    if workspace::load_panta_project(&project).map(|p| p.rhei_ids.is_empty()).unwrap_or(false) {
        println!();
        println!("{}", add_a_rhei_help());
    }
    Ok(())
}

/// Nearest ancestor (strictly above `dir`) that is a Panta project root.
fn enclosing_panta_project(dir: &Path) -> Option<PathBuf> {
    let start = rhei_core::platform::canonical_path(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut current = start.parent();
    while let Some(dir) = current {
        if dir.join("index.panta.md").is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Default title from the host directory name: `-`/`_` become spaces and
/// each word is capitalized (`my-project` → `My Project`). §FS-rhei-init.1
fn default_project_title(dir: &Path) -> String {
    let name = rhei_core::platform::canonical_path(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Project")
        .to_string();
    name.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Append missing entries to `dir/.gitignore`, creating the file when absent
/// and never rewriting entries already present. `true` when it changed, so the
/// caller can name what init touched. §FS-rhei-init.3 §FS-rhei-init.5
fn seed_gitignore(dir: &Path, entries: &[&str]) -> MietteResult<bool> {
    let path = dir.join(".gitignore");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let missing: Vec<&str> = entries
        .iter()
        .copied()
        .filter(|entry| !existing.lines().any(|line| line.trim() == *entry))
        .collect();
    if missing.is_empty() {
        return Ok(false);
    }
    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str("# Rhei\n");
    for entry in missing {
        out.push_str(entry);
        out.push('\n');
    }
    fs::write(&path, out).map_err(|err| file_io_report(&path, "failed to write", err))?;
    Ok(true)
}

/// Create or update the marked Rhei block in the host's `AGENTS.md`, stripping
/// every trace of a previous note first so it stays idempotent. `true` when it
/// changed, so the caller can name it. §FS-rhei-init.4 §FS-rhei-init.5
fn write_agents_note(host: &Path, here: bool) -> MietteResult<(bool, PathBuf)> {
    // §FS-rhei-init.4: anchored at the host, the only directory the user named
    // — an earlier revision walked up to the enclosing repository root and
    // appended to a tracked instruction file nobody had asked it to touch.
    let path = agents_note_target(host);
    let location = if here {
        "This directory is a Rhei (Panta) project."
    } else {
        "The Rhei (Panta) project for this repository lives in `panta/`."
    };
    let block = format!(
        "{AGENTS_NOTE_BEGIN}\n## Rhei\n\n{location} {AGENTS_NOTE_TAIL}\n{AGENTS_NOTE_END}\n"
    );
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let cleaned = strip_rhei_note(&existing);
    let updated = if cleaned.trim().is_empty() {
        block
    } else {
        let mut out = cleaned.trim_end().to_string();
        out.push_str("\n\n");
        out.push_str(&block);
        out
    };
    if updated == existing {
        return Ok((false, path));
    }
    fs::write(&path, updated)
        .map_err(|err| file_io_report(&path, "failed to write", err))?;
    Ok((true, path))
}

/// Which instruction file in `dir` receives the agent-discovery note.
///
/// `AGENTS.md` is the canonical target, but a directory whose agent
/// instructions live only in `CLAUDE.md` has an agent that never opens
/// `AGENTS.md` — the note must land in the file that agent actually reads.
/// A file already carrying the note wins outright so a re-run rewrites in
/// place instead of duplicating the note into a newer sibling.
// §FS-rhei-init.4: CLAUDE.md-only repositories get the note in CLAUDE.md.
fn agents_note_target(dir: &Path) -> PathBuf {
    let agents = dir.join("AGENTS.md");
    let claude = dir.join("CLAUDE.md");
    if carries_rhei_note(&agents) {
        return agents;
    }
    if carries_rhei_note(&claude) {
        return claude;
    }
    if !agents.exists() && claude.exists() {
        return claude;
    }
    agents
}

/// Whether `path` already holds an agent-discovery note this init would rewrite.
fn carries_rhei_note(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|content| strip_rhei_note(&content) != content)
}

/// The enclosing git repository root, if `dir` is inside one.
fn repository_root(dir: &Path) -> Option<PathBuf> {
    let start = rhei_core::platform::canonical_path(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut current: Option<&Path> = Some(start.as_path());
    while let Some(candidate) = current {
        if candidate.join(".git").exists() {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

/// A host inside a repository it does not own gets a *hint*, never a write:
/// an agent starting at the enclosing root will not see a note that lives in
/// the host, and only the user can decide to point it there. Printed whenever
/// the layout calls for it, not only when the note changed — it describes
/// where the project sits, not a file init touched.
///
/// Three things silence it: no enclosing repository, a host that *is* the
/// root (nothing above it to point from), and a root whose instruction file
/// already reads as carrying a Rhei note — plus `--no-agents` at the call
/// site, where no note is written to advertise. The middle two overlap for a
/// host that is its own root, since the note just written is in that root's
/// own file; each is kept for the condition it states. §FS-rhei-init.4
fn report_enclosing_repository_hint(host: &Path) {
    let Some(root) = repository_root(host) else {
        return;
    };
    // `same_path` rather than `==`: `root` comes back canonicalized and `host`
    // is whatever the caller spelled — the same directory in different words on
    // every platform, and visibly so on Windows.
    if same_path(&root, host) {
        return;
    }
    // Anything that reads as a Rhei note silences this — init will not judge
    // whose it is, in a file it has promised not to write. It stays put:
    // removing it is the user's call, in the user's file. §FS-rhei-init.4
    let target = agents_note_target(&root);
    if carries_rhei_note(&target) {
        return;
    }
    // Absolute, not shortened: the whole point of the line is that this file
    // is in a *different* directory from the host, and a bare `AGENTS.md`
    // relative to the invocation directory reads as the one just written.
    println!(
        "Hint: init only writes files it chose inside the host directory. Add a pointer in {} \
         yourself if agents starting at the enclosing repository root should find this project.",
        target.display()
    );
}

/// Name the host files init changed and state the gitignore consequence. Doing
/// this silently is how a team learns weeks later that no plan was ever
/// committed. §FS-rhei-init.5
fn report_host_changes(changed: &[String], here: bool) {
    if !changed.is_empty() {
        println!("Also changed in the host directory: {}", changed.join(", "));
    }
    if !here && changed.iter().any(|name| name == ".gitignore") {
        println!(
            "Note: `panta/` is gitignored — planning state is working material, not \
             repository content. Delete that entry to version the project."
        );
    }
}

/// How far the block opened by a begin marker reaches, as an offset into the
/// lines that follow it — `None` when that begin marker is orphaned.
///
/// The search stops at whichever marker comes first, and only an end marker
/// closes the region: a second begin marker before any end means the first is
/// orphaned, because the end that eventually arrives belongs to the second.
/// Scanning past it would pair the outermost begin with the innermost end and
/// delete the user content a merge stranded in between. §FS-rhei-init.4
fn closing_end_marker(after_begin: &[&str]) -> Option<usize> {
    let at = after_begin.iter().position(|line| {
        let trimmed = line.trim();
        trimmed == AGENTS_NOTE_END || trimmed == AGENTS_NOTE_BEGIN
    })?;
    (after_begin[at].trim() == AGENTS_NOTE_END).then_some(at)
}

/// Remove every trace of a previously written agent note: marker-delimited
/// regions, orphaned markers, and a marker-less `## Rhei` section that still
/// carries the note body (a merge may have eaten the markers). §FS-rhei-init.4
fn strip_rhei_note(existing: &str) -> String {
    const SENTINEL: &str = "Rhei (Panta) project";
    let lines: Vec<&str> = existing.lines().collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == AGENTS_NOTE_BEGIN {
            // §FS-rhei-init.4: an orphaned begin marker (end marker lost) is
            // removed alone — the lines after it are user content, not the note.
            match closing_end_marker(&lines[i + 1..]) {
                Some(end) => i += end + 2, // past the block and its end marker
                None => i += 1,
            }
            continue;
        }
        if trimmed == AGENTS_NOTE_END {
            i += 1;
            continue;
        }
        if trimmed == "## Rhei" {
            let mut j = i + 1;
            let mut has_sentinel = false;
            while j < lines.len()
                && !lines[j].starts_with("## ")
                && lines[j].trim() != AGENTS_NOTE_BEGIN
            {
                // §FS-rhei-init.4: the note's prose is one paragraph, so the
                // blank line closing it ends the section — running on to the
                // end of the file would take the user's own tail with it.
                if has_sentinel && lines[j].trim().is_empty() {
                    break;
                }
                has_sentinel |= lines[j].contains(SENTINEL);
                j += 1;
            }
            if has_sentinel {
                i = j;
                continue;
            }
        }
        out.push(lines[i]);
        i += 1;
    }
    let mut result = out.join("\n");
    if existing.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

/// Load the fresh project and say where it lives and what it contains; a
/// discovery failure is a warning, not an init failure, so init doubles as a
/// first validation of pre-existing plans. §FS-rhei-init.5
fn report_initialized_project(project: &Path, title: &str, here: bool) {
    let location = if here { String::new() } else { " at panta/".to_string() };
    match workspace::load_panta_project(project) {
        Ok(loaded) if loaded.rhei_ids.is_empty() => {
            println!("Initialized Panta project \"{title}\"{location} with no rheis yet.");
        }
        Ok(loaded) => {
            let noun = if loaded.rhei_ids.len() == 1 { "rhei" } else { "rheis" };
            println!(
                "Initialized Panta project \"{}\"{} with {} {}: {}",
                title,
                location,
                loaded.rhei_ids.len(),
                noun,
                loaded.rhei_ids.join(", ")
            );
        }
        Err(err) => {
            println!("Initialized Panta project \"{title}\"{location}.");
            eprintln!("warning: the new project does not load cleanly: {}", err.message);
        }
    }
}
