    // The target of a `--into`: which rhei, which parent task, which machine
    // file the union is written into, and the hold taken on all of it.
    // §FS-rhei-library.2 §FS-rhei-library.2.1

    /// A rhei a union can be written into, in whichever layout it uses.
    struct UnionHost {
        /// The rhei's execution root, which is what `states.yaml` sits in.
        root: PathBuf,
        /// The file the plan's header and frontmatter live in.
        index: PathBuf,
        /// True when the rhei is one `.rhei.md` rather than a workspace.
        single_file: bool,
        machine: PathBuf,
        /// The parent task placed tickets go under, rhei-local.
        parent: Option<String>,
        /// The Panta project the rhei is a member of, which is the settings
        /// root it resolves and the scope it is validated in.
        /// §FS-rhei-templates.6.2
        project: Option<PathBuf>,
    }

    /// What a `--into` names, decided by what is on disk rather than by how it
    /// is spelled: a rhei, perhaps with a task in it, or a Panta project.
    /// §FS-rhei-library.2.2
    enum IntoTarget {
        Rhei(UnionHost),
        Project(PathBuf),
    }

    /// Resolve `--into <target>` from the working directory. §FS-rhei-library.2.2
    fn resolve_into_target(target: &str) -> MietteResult<IntoTarget> {
        let cwd = std::env::current_dir().map_err(|err| {
            miette!(help = cwd_help(), "failed to determine working directory: {err}")
        })?;
        resolve_into_target_from(&cwd, target)
    }

    /// A bare id is one path segment not beginning with `.`, and only a bare id
    /// is split into `<rhei>.<task>`; anything else is a path, never split,
    /// since a rhei id is a single segment. §FS-rhei-library.2.2 §FS-rhei-panta.2
    fn is_bare_into_id(target: &str) -> bool {
        !target.is_empty()
            && !target.starts_with('.')
            && !target.contains(['/', '\\'])
            && !Path::new(target).is_absolute()
    }

    /// The five rules of §FS-rhei-library.2.2, in order: the basin is refused,
    /// a bare id keeps its split and is looked up at the candidate roots a rhei
    /// target has always had, a path is exactly one thing or none, and a bare id
    /// naming a rhei at one root and a project at another is refused naming
    /// both. `rhei new --under` resolves a parent the same way. §FS-rhei-new.3
    fn resolve_into_target_from(cwd: &Path, target: &str) -> MietteResult<IntoTarget> {
        if !is_bare_into_id(target) {
            return resolve_into_path(cwd, target);
        }
        let (rhei_id, parent) = match target.split_once('.') {
            Some((rhei_id, parent)) if !parent.is_empty() => (rhei_id, Some(parent.to_owned())),
            _ => (target, None),
        };
        if rhei_id == rhei_core::workspace::BASIN_RHEI_ID {
            return Err(basin_is_never_a_target());
        }
        let roots = union_candidate_roots(cwd, rhei_id);
        let rhei = roots
            .iter()
            .find(|root| root.join("index.rhei.md").is_file())
            .map(|root| workspace_host(root, parent.clone()))
            .or_else(|| {
                let plan = union_candidate_plans(cwd, rhei_id).into_iter().find(|plan| plan.is_file())?;
                Some(single_file_host(plan, cwd, parent.clone()))
            });
        let project = roots.iter().find(|root| workspace::is_panta_project(root));
        match (rhei, project) {
            (Some(host), Some(project)) => Err(ambiguous_into_target(target, &host, project)),
            (Some(host), None) => Ok(IntoTarget::Rhei(host)),
            (None, Some(project)) => match parent {
                Some(task) => Err(project_has_no_tasks(target, project, &task)),
                None => Ok(IntoTarget::Project(project.clone())),
            },
            (None, None) => Err(no_rhei_to_place_into(rhei_id)),
        }
    }

    /// A path target: a directory holding `index.rhei.md` is a rhei, one
    /// holding `index.panta.md` is a project, and anything else is nothing to
    /// place into. §FS-rhei-library.2.2
    fn resolve_into_path(cwd: &Path, target: &str) -> MietteResult<IntoTarget> {
        let root = into_target_path(cwd, target);
        let in_project = root.parent().is_some_and(workspace::is_panta_project);
        if in_project && root.file_name() == Some(std::ffi::OsStr::new(rhei_core::workspace::BASIN_RHEI_ID)) {
            return Err(basin_is_never_a_target());
        }
        let rhei = root.join("index.rhei.md").is_file();
        match (rhei, workspace::is_panta_project(&root)) {
            (true, true) => Err(ambiguous_into_target(target, &workspace_host(&root, None), &root)),
            (true, false) => Ok(IntoTarget::Rhei(workspace_host(&root, None))),
            (false, true) => Ok(IntoTarget::Project(root)),
            (false, false) => Err(match dotted_path_hint(cwd, target) {
                Some(hint) => miette!(help = hint, "no rhei '{target}' to place into"),
                None => no_rhei_to_place_into(target),
            }),
        }
    }

    /// A path is never split at a dot, so `panta/reports.ticket` names a
    /// directory called `reports.ticket`. When the part of its last segment
    /// before the dot is a rhei in that directory, say so and give the bare
    /// spelling that does split; for a segment that is a single-file rhei's
    /// plan, `reports.rhei.md`, that spelling is the rhei's bare id.
    /// §FS-rhei-library.2.2
    fn dotted_path_hint(cwd: &Path, target: &str) -> Option<String> {
        let (holder, last) = target.trim_end_matches(['/', '\\']).rsplit_once(['/', '\\'])?;
        // A single-file rhei's plan names the rhei itself, never a task of it.
        let plan_file = last.strip_suffix(".rhei.md");
        let (rhei_id, task) = match plan_file {
            Some(rhei_id) => (rhei_id, ""),
            None => last.split_once('.')?,
        };
        if rhei_id.is_empty() || (plan_file.is_none() && task.is_empty()) {
            return None;
        }
        let dir = into_target_path(cwd, if holder.is_empty() { "/" } else { holder });
        let rhei = dir.join(rhei_id).join("index.rhei.md").is_file()
            || dir.join(format!("{rhei_id}.rhei.md")).is_file();
        if !rhei {
            return None;
        }
        let from = if dir == cwd { "this directory".to_owned() } else { format!("'{holder}'") };
        let anywhere = if workspace::is_panta_project(&dir) {
            ", or from anywhere inside its project"
        } else {
            ""
        };
        if plan_file.is_some() {
            return Some(format!(
                "'{target}' names the plan file of rhei '{rhei_id}', and `--into` names a rhei \
                 by its id: name it bare, `--into {rhei_id}`, from {from}{anywhere}."
            ));
        }
        Some(format!(
            "a path is never split at a dot, so '{target}' names a directory called '{last}'. \
             To place under task '{task}' of rhei '{rhei_id}', name it bare, `--into \
             {rhei_id}.{task}`, from {from}{anywhere}."
        ))
    }

    /// `cwd` joined with `target`, `.` and `..` folded away lexically, so `.`
    /// names the working directory itself rather than a directory called `.`.
    fn into_target_path(cwd: &Path, target: &str) -> PathBuf {
        let mut path = PathBuf::new();
        for component in cwd.join(target).components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    path.pop();
                }
                other => path.push(other.as_os_str()),
            }
        }
        path
    }

    fn workspace_host(root: &Path, parent: Option<String>) -> UnionHost {
        UnionHost {
            index: root.join("index.rhei.md"),
            machine: root.join("states.yaml"),
            project: union_project(root, false),
            root: root.to_path_buf(),
            single_file: false,
            parent,
        }
    }

    fn single_file_host(plan: PathBuf, cwd: &Path, parent: Option<String>) -> UnionHost {
        let root = plan.parent().unwrap_or(cwd).to_path_buf();
        UnionHost {
            index: plan,
            machine: root.join("states.yaml"),
            project: union_project(&root, true),
            root,
            single_file: true,
            parent,
        }
    }

    fn basin_is_never_a_target() -> Report {
        miette!(
            help = "file the ticket into a rhei of its own, or `rhei new --under <rhei>`; \
                    the basin's tickets run under the project default.",
            "the basin is never a `--into` target: it holds unfiled tickets that run under \
             the project default and has no machine of its own to add to"
        )
    }

    fn no_rhei_to_place_into(target: &str) -> Report {
        miette!(
            help = "`--into` names a rhei or a Panta project that already exists; list the \
                    rheis with `rhei list`.",
            "no rhei '{target}' to place into"
        )
    }

    /// A bare id that finds both a rhei and a project is never resolved by
    /// preference: guessing wrong writes into the wrong one. The refusal names
    /// both and the path spelling that says each. §FS-rhei-library.2.2
    fn ambiguous_into_target(target: &str, rhei: &UnionHost, project: &Path) -> Report {
        let rhei_spelling = if rhei.single_file {
            format!(
                "'{}' is a single-file rhei, which only its id names, so rename one of them \
                 to place into the rhei",
                display_slash(&rhei.index)
            )
        } else {
            format!("`--into {}` for the rhei", path_spelling(&rhei.root))
        };
        miette!(
            help = format!(
                "say which one with a path: {rhei_spelling}, `--into {}` for the project.",
                path_spelling(project)
            ),
            "`--into {target}` names two different things\n  a rhei:    {}\n  a project: {}",
            display_slash(&rhei.index),
            display_slash(&project.join(workspace::PANTA_INDEX_FILE))
        )
    }

    /// A directory spelled so that `--into` reads it as a path: a single
    /// segment would be a bare id again, so it gains a leading `./`.
    fn path_spelling(dir: &Path) -> String {
        let shown = display_slash(dir);
        if is_bare_into_id(&shown) { format!("./{shown}") } else { shown }
    }

    /// A project has no task tree, so there is no task in it to place under.
    /// §FS-rhei-library.2.2
    fn project_has_no_tasks(target: &str, project: &Path, task: &str) -> Report {
        let shown = path_spelling(project);
        miette!(
            help = format!(
                "name the project alone, `--into {shown}`, or a member rhei inside it, \
                 `--into {shown}/<member>`."
            ),
            "`--into {target}` names task '{task}' in the Panta project at '{}', but a project \
             has no tasks to place under",
            display_slash(project)
        )
    }

    /// Where a directory-workspace rhei named `rhei_id` could be: beside the
    /// working directory, or in the project that encloses it.
    fn union_candidate_roots(cwd: &Path, rhei_id: &str) -> Vec<PathBuf> {
        let mut roots = vec![cwd.join(rhei_id)];
        if let Some(project) = enclosing_project_for_new_rhei(cwd) {
            roots.push(project.join(rhei_id));
        }
        roots
    }

    /// The same for a single-file rhei.
    fn union_candidate_plans(cwd: &Path, rhei_id: &str) -> Vec<PathBuf> {
        let mut plans = vec![cwd.join(format!("{rhei_id}.rhei.md"))];
        if let Some(project) = enclosing_project_for_new_rhei(cwd) {
            plans.push(project.join(format!("{rhei_id}.rhei.md")));
        }
        plans
    }

    /// The cases of §FS-rhei-library.2.1, decided before anything is rendered.
    ///
    /// Under §FS-rhei-plan-language.1.3 clause 1 the target's own root is its
    /// machine, so writing the union into the root file is the whole of it and
    /// `--into` has nothing to declare. What survives is one refusal — no file
    /// in the root — behind the parse that refuses an index still carrying the
    /// retired `**States:**` line before anything is written.
    /// §FS-rhei-plan-language.2.2
    fn resolve_host_machine(host: &UnionHost) -> MietteResult<()> {
        let index = fs::read_to_string(&host.index)
            .map_err(|err| file_io_report(&host.index, "failed to read the target's index", err))?;
        let parsed = if host.single_file {
            rhei_core::parse(&index).map(drop)
        } else {
            rhei_core::parser::parse_workspace_index(&index).map(drop)
        };
        parsed.map_err(|err| parse_report(&host.index, &index, &err))?;
        if !host.machine.is_file() {
            return Err(no_machine_of_its_own(host));
        }
        Ok(())
    }

    /// The refusal for a rhei with no machine of its own, with the **one**
    /// remedy that makes it eligible: the copy is the whole of it now that the
    /// rhei's own root is what resolution reads. A declaration printed beside
    /// it would ask the reader to write a line that is going away.
    /// §FS-rhei-library.2.1
    fn no_machine_of_its_own(host: &UnionHost) -> Report {
        let machine = display_slash(&host.machine);
        let project = host
            .root
            .parent()
            .and_then(enclosing_project_for_new_rhei)
            .map_or_else(|| "<project>".to_owned(), |dir| display_slash(&dir));
        miette!(
            help = format!(
                "give the rhei a machine of its own:\n  \
                 cp {project}/states.yaml {machine}\n\
                 The rhei then stops following the project default: what the project changes \
                 later no longer reaches it."
            ),
            "'{}' has no states.yaml of its own, so a union written there would be inert",
            display_slash(&host.root)
        )
    }

    /// Every ticket id the target already holds, rhei-local, and the file each
    /// one lives in. §FS-rhei-library.4
    fn host_ticket_files(host: &UnionHost) -> MietteResult<Vec<(PathBuf, Vec<String>)>> {
        if host.single_file {
            let raw = fs::read_to_string(&host.index)
                .map_err(|err| file_io_report(&host.index, "failed to read the target", err))?;
            return Ok(vec![(host.index.clone(), declared_ids(&raw))]);
        }
        let tasks = host.root.join("tasks");
        if !tasks.is_dir() {
            return Ok(Vec::new());
        }
        let mut paths: Vec<PathBuf> = fs::read_dir(&tasks)
            .map_err(|err| file_io_report(&tasks, "failed to read the target's tasks", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .collect();
        paths.sort();
        let mut files = Vec::new();
        for path in paths {
            let raw = fs::read_to_string(&path)
                .map_err(|err| file_io_report(&path, "failed to read a task file", err))?;
            files.push((path, declared_ids(&raw)));
        }
        Ok(files)
    }

    /// The next three-digit number a new task file in the target takes.
    /// §FS-rhei-new.3.1
    fn next_task_number(files: &[(PathBuf, Vec<String>)]) -> u32 {
        files
            .iter()
            .filter_map(|(path, _)| {
                let stem = path.file_stem()?.to_str()?;
                let digits: String = stem.chars().take_while(char::is_ascii_digit).collect();
                digits.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0)
            + 1
    }

    /// The host as the union read it, under whatever hold the mode takes.
    ///
    /// A union is a read-splice-write of whole files, so the hold has to start
    /// before the first host read: `new_lock.rs` documents the exact loss
    /// otherwise — a write that splices a whole file while a completion
    /// rewrites a `**State:**` line in it silently drops the completion.
    /// `Drop` releases every lock on every exit path.
    /// §FS-rhei-library.2 §FS-rhei-new.4
    struct HeldHost {
        /// Every ticket id the target holds, and the file each lives in.
        files: Vec<(PathBuf, Vec<String>)>,
        /// The scope lock first, then one per destination, in the order they
        /// were taken. Held for the struct's lifetime and never read.
        _locks: Vec<NewCreateLock>,
    }

    /// Read the host under the permanent sibling lock every rewriting command
    /// takes: the scope lock, then one destination lock per file the placement
    /// rewrites. The scope is `union_scope_root`'s, so a member's lock covers
    /// the project settings the hoist rewrites as well as the rhei itself.
    ///
    /// Only a `--into` takes them. An `includes:` entry unions into a freshly
    /// rendered tree nothing else can see, and a sidecar written there would
    /// travel into the host with the rest of the bundle; `--dry-run` writes
    /// nothing at all. §FS-rhei-new.4 §FS-rhei-library.7.1
    fn hold_host(
        host: &UnionHost,
        tickets: &[PartTickets],
        mode: UnionMode,
    ) -> MietteResult<HeldHost> {
        if mode != UnionMode::Place {
            return Ok(HeldHost { files: host_ticket_files(host)?, _locks: Vec::new() });
        }
        let scope = lock_new_create(union_scope_root(host))?;
        let files = host_ticket_files(host)?;
        let mut locks = vec![scope];
        for destination in union_destinations(host, &files, tickets) {
            if locks.iter().any(|lock| same_path(lock.path(), &destination)) {
                continue;
            }
            prepare_destination_lock_parent(&destination)?;
            if let Some(lock) = lock_new_destination(&locks[0], &destination)? {
                locks.push(lock);
            }
        }
        Ok(HeldHost { files, _locks: locks })
    }

    /// The plan files a placement writes. The machine is not among them: no
    /// other command rewrites `states.yaml`, and the scope lock already
    /// serializes this union against another. §FS-rhei-new.3.1
    fn union_destinations(
        host: &UnionHost,
        host_files: &[(PathBuf, Vec<String>)],
        tickets: &[PartTickets],
    ) -> Vec<PathBuf> {
        let mut destinations = vec![host.index.clone()];
        if host.single_file {
            return destinations;
        }
        match host.parent.as_deref() {
            Some(parent) => destinations.extend(
                host_files
                    .iter()
                    .find(|(_, ids)| ids.iter().any(|id| id == parent))
                    .map(|(path, _)| path.clone()),
            ),
            None => {
                let mut number = next_task_number(host_files);
                for file in tickets {
                    let name = format!("{number:03}-{}.md", file.slug);
                    destinations.push(host.root.join("tasks").join(name));
                    number += 1;
                }
            }
        }
        destinations
    }
