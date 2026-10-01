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

    /// Resolve `--into <rhei>[.<task>]` the way `rhei new --under` resolves a
    /// parent: the first segment names the rhei, the rest names a task in it.
    /// §FS-rhei-new.3 §FS-rhei-library.2
    fn resolve_union_host(target: &str) -> MietteResult<UnionHost> {
        let (rhei_id, parent) = match target.split_once('.') {
            Some((rhei_id, parent)) if !parent.is_empty() => (rhei_id, Some(parent.to_owned())),
            _ => (target, None),
        };
        if rhei_id == rhei_core::workspace::BASIN_RHEI_ID {
            return Err(miette!(
                help = "file the ticket into a rhei of its own, or `rhei new --under <rhei>`; \
                        the basin's tickets run under the project default.",
                "the basin is never a `--into` target: it holds unfiled tickets that run under \
                 the project default and has no machine of its own to add to"
            ));
        }
        let cwd = std::env::current_dir().map_err(|err| {
            miette!(help = cwd_help(), "failed to determine working directory: {err}")
        })?;
        let roots = union_candidate_roots(&cwd, rhei_id);
        let workspace = roots.iter().find(|root| root.join("index.rhei.md").is_file());
        if let Some(root) = workspace {
            return Ok(UnionHost {
                index: root.join("index.rhei.md"),
                machine: root.join("states.yaml"),
                project: union_project(root, false),
                root: root.clone(),
                single_file: false,
                parent,
            });
        }
        let plan = union_candidate_plans(&cwd, rhei_id).into_iter().find(|plan| plan.is_file());
        if let Some(plan) = plan {
            let root = plan.parent().unwrap_or(&cwd).to_path_buf();
            return Ok(UnionHost {
                index: plan,
                machine: root.join("states.yaml"),
                project: union_project(&root, true),
                root,
                single_file: true,
                parent,
            });
        }
        Err(miette!(
            help = "`--into` names a rhei that already exists; list them with `rhei list`.",
            "no rhei '{rhei_id}' to place into"
        ))
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

    /// The three cases of §FS-rhei-library.2.1, decided before anything is
    /// rendered.
    ///
    /// Under §FS-rhei-plan-language.1.3 clause 1 the target's own root is
    /// consulted whatever its index says, so writing the union into the root
    /// file is the whole of it and `--into` has nothing left to declare. What
    /// survives is the pair of refusals: no file in the root, and an index
    /// naming a machine that file is not.
    fn resolve_host_machine(host: &UnionHost) -> MietteResult<()> {
        let index = fs::read_to_string(&host.index)
            .map_err(|err| file_io_report(&host.index, "failed to read the target's index", err))?;
        let declared = index
            .lines()
            .find_map(|line| line.trim().strip_prefix("**States:**"))
            .map(|name| name.trim().to_owned());
        if !host.machine.is_file() {
            return Err(no_machine_of_its_own(host));
        }
        let text = fs::read_to_string(&host.machine).map_err(|err| {
            file_io_report(&host.machine, "failed to read the target's states", err)
        })?;
        let name = serde_yaml::from_str::<YamlValue>(&text)
            .ok()
            .and_then(|value| value.get("name").and_then(YamlValue::as_str).map(str::to_owned))
            .ok_or_else(|| {
                miette!(
                    help = "a state machine names itself with a top-level `name:`.",
                    "'{}' declares no machine name",
                    display_slash(&host.machine)
                )
            })?;
        match declared {
            Some(declared) if declared == name => Ok(()),
            Some(declared) => Err(miette!(
                help = format!(
                    "make them agree: either declare `**States:** {name}` in the index, or \
                     rename the machine in {}.",
                    display_slash(&host.machine)
                ),
                "'{}' declares `**States:** {declared}`, but '{}' is named '{name}'",
                display_slash(&host.index),
                display_slash(&host.machine)
            )),
            // A silent index is now the ordinary case: clause 1 reads the file
            // the union lands in, so the target already runs under it.
            None => Ok(()),
        }
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
