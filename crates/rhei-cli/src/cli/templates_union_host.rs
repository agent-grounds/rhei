    // The target of a `--into`: which rhei, which parent task, and which
    // machine file the union is written into.
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

    /// What `--into` must do about the target's `**States:**` declaration
    /// before it writes a union into a file the rhei may not actually run
    /// under. §FS-rhei-library.2.1
    enum MachineDeclaration {
        /// Declared and matching: union in, write no declaration.
        Matches,
        /// A root file with a silent index: write the declaration too. This is
        /// the interim clause of the slice that introduces `--into`.
        Write(String),
    }

    /// The four cases of [§FS-rhei-library.2.1], decided before anything is
    /// rendered.
    fn resolve_host_machine(host: &UnionHost) -> MietteResult<MachineDeclaration> {
        let index = fs::read_to_string(&host.index)
            .map_err(|err| file_io_report(&host.index, "failed to read the target's index", err))?;
        let declared = index
            .lines()
            .find_map(|line| line.trim().strip_prefix("**States:**"))
            .map(|name| name.trim().to_owned());
        if !host.machine.is_file() {
            return Err(no_machine_of_its_own(host, declared.as_deref()));
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
                    display_path(&host.machine).display()
                )
            })?;
        match declared {
            Some(declared) if declared == name => Ok(MachineDeclaration::Matches),
            Some(declared) => Err(miette!(
                help = format!(
                    "make them agree: either declare `**States:** {name}` in the index, or \
                     rename the machine in {}.",
                    display_path(&host.machine).display()
                ),
                "'{}' declares `**States:** {declared}`, but '{}' is named '{name}'",
                display_path(&host.index).display(),
                display_path(&host.machine).display()
            )),
            None => Ok(MachineDeclaration::Write(name)),
        }
    }

    /// The refusal for a rhei with no machine of its own, with **both**
    /// remedies: while resolution is today's, the copy alone changes nothing.
    /// §FS-rhei-library.2.1
    fn no_machine_of_its_own(host: &UnionHost, declared: Option<&str>) -> Report {
        let machine = display_path(&host.machine).display().to_string();
        let project = host
            .root
            .parent()
            .and_then(enclosing_project_for_new_rhei)
            .map_or_else(|| "<project>".to_owned(), |dir| display_path(&dir).display().to_string());
        let declaration = declared.map_or("<name>", |name| name);
        miette!(
            help = format!(
                "give the rhei a machine of its own, then declare it:\n  \
                 cp {project}/states.yaml {machine}\n  \
                 **States:** {declaration}   (add this line to {})\n\
                 The rhei then stops following the project default: what the project changes \
                 later no longer reaches it.",
                display_path(&host.index).display()
            ),
            "'{}' has no states.yaml of its own, so a union written there would be inert",
            display_path(&host.root).display()
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
