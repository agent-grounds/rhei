fn completion_plan_path() -> Option<PathBuf> {
    let words = completion_words();
    let command = completion_command_name(&words)?;
    // `rhei new` spends its positional on the title, not a path: reading it as
    // one would complete every id against a plan that does not exist.
    // §FS-rhei-new.1.1
    if command == "new" {
        return completion_option_value("project")
            .map(PathBuf::from)
            .or_else(|| resolve_plan_target(None).ok().map(|target| target.path));
    }
    // §FS-rhei-panta.6: with the plan positional omitted, complete task and
    // rhei ids against the same target the command itself would resolve.
    first_command_positional(&words, &command)
        .map(PathBuf::from)
        .or_else(|| resolve_plan_target(None).ok().map(|target| target.path))
}

fn completion_command_name(words: &[String]) -> Option<String> {
    let mut expect_value = false;
    for word in words.iter().skip(1) {
        if word.is_empty() {
            break;
        }
        if expect_value {
            expect_value = false;
            continue;
        }
        if let Some(option) = word.strip_prefix("--") {
            if option.split_once('=').is_none() && option == "state-machine" {
                expect_value = true;
            }
            continue;
        }
        return Some(word.clone());
    }
    None
}

fn first_command_positional(words: &[String], command: &str) -> Option<String> {
    let command_index = words.iter().position(|word| word == command)?;
    let mut expect_value_for: Option<&str> = None;
    for word in words.iter().skip(command_index + 1) {
        if word.is_empty() {
            break;
        }
        if let Some(option) = expect_value_for.take() {
            if option != "set" && option != "set-file" && option != "values" && option != "output" {
                continue;
            }
            continue;
        }
        if let Some(option) = word.strip_prefix("--") {
            if let Some((_, _)) = option.split_once('=') {
                continue;
            }
            if matches!(
                option,
                "reason"
                    | "task"
                    | "from"
                    | "to"
                    | "supervisor"
                    | "result"
                    | "set"
                    | "set-file"
                    | "values"
                    | "output"
                    | "agent"
                    | "agent-mode"
                    | "model"
                    | "program-timeout"
                    | "parallel"
                    | "state-machine"
                    | "state"
                    | "assignee"
                    | "kind"
                    | "has-prior"
                    | "parent"
                    | "contains"
                    | "limit"
            ) {
                expect_value_for = Some(option);
            }
            continue;
        }
        return Some(word.clone());
    }
    None
}

fn completion_option_value(name: &str) -> Option<String> {
    let words = completion_words();
    let flag = format!("--{name}");
    let prefix = format!("--{name}=");
    let mut iter = words.iter().peekable();
    while let Some(word) = iter.next() {
        if let Some(value) = word.strip_prefix(&prefix) {
            return Some(value.to_string());
        }
        if word == &flag {
            return iter.peek().filter(|value| !value.is_empty()).map(|value| (*value).clone());
        }
    }
    None
}

fn completion_words() -> Vec<String> {
    let args = std::env::args_os().collect::<Vec<_>>();
    let start = args.iter().position(|arg| arg == "--").map(|idx| idx + 1).unwrap_or(1);
    args.into_iter().skip(start).map(|arg| arg.to_string_lossy().to_string()).collect()
}

fn flatten_tasks(rhei: &rhei_core::ast::Rhei) -> Vec<&rhei_core::ast::Task> {
    fn collect<'a>(task: &'a rhei_core::ast::Task, tasks: &mut Vec<&'a rhei_core::ast::Task>) {
        tasks.push(task);
        for child in &task.children {
            collect(child, tasks);
        }
    }

    let mut tasks = Vec::new();
    for task in &rhei.tasks {
        collect(task, &mut tasks);
    }
    tasks
}

fn current_task_state(plan: &Path, task_id: &str) -> MietteResult<String> {
    let loaded = load_plan(plan)?;
    flatten_tasks(&loaded.rhei)
        .into_iter()
        .find(|task| task.id.to_string() == task_id)
        .map(|task| task.state.clone())
        .ok_or_else(|| miette!(
            help = task_id_help(),
            "task '{}' not found in {}", task_id, plan.display()
        ))
}

fn xdg_data_home() -> MietteResult<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Ok(home_dir()?.join(".local/share")),
    }
}

fn xdg_config_home() -> MietteResult<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Ok(home_dir()?.join(".config")),
    }
}

/// Load a [`rhei_validator::StateMachine`] from the user-provided path, or fall back to the
/// built-in default when no path was given.
fn load_state_machine(path: Option<&Path>) -> MietteResult<rhei_validator::StateMachine> {
    match path {
        // §FS-rhei-errors.1.2: a bad state machine says which file to edit and
        // how to re-check it, not just that loading failed.
        Some(p) => rhei_validator::StateMachine::from_yaml_file(p)
            .map_err(|err| state_machine_load_report(p, err)),
        None => Ok(rhei_validator::StateMachine::builtin_default()),
    }
}

#[derive(Clone)]
struct ResolvedStateMachine {
    machine: rhei_validator::StateMachine,
    path: Option<PathBuf>,
}

/// Every machine governing a loaded plan, with the file each resolved from:
/// the project default plus one entry per self-declaring rhei. A ticket's
/// machine — and the callback base its transitions run under — resolves
/// through its owning rhei.
// §DA-per-rhei-state-machines §AR-rhei-panta.4
struct ResolvedMachineSet {
    default: ResolvedStateMachine,
    per_rhei: BTreeMap<String, ResolvedStateMachine>,
}

/// One state-machine source used by a validation pass, with the rheis that it
/// governs. Source identity is the resolved path, not machine contents.
// §FS-rhei-validate.6
#[derive(Clone)]
struct ValidationMachineSource {
    path: Option<PathBuf>,
    project_default: bool,
    rheis: Vec<String>,
}

impl ResolvedMachineSet {
    fn single(default: ResolvedStateMachine) -> Self {
        Self { default, per_rhei: BTreeMap::new() }
    }

    /// The resolved machine governing the ticket named by a project-qualified
    /// id string (`auth.1`): its owning rhei's machine, else the default.
    fn for_task_str(&self, task_id: &str) -> &ResolvedStateMachine {
        let rhei_id = task_id.split('.').next().unwrap_or(task_id);
        self.per_rhei.get(rhei_id).unwrap_or(&self.default)
    }

    fn machine_for_task_str(&self, task_id: &str) -> &rhei_validator::StateMachine {
        &self.for_task_str(task_id).machine
    }

    /// The validator-facing set (owned clone).
    fn validator_set(&self) -> rhei_validator::MachineSet {
        rhei_validator::MachineSet {
            default: self.default.machine.clone(),
            per_rhei: self
                .per_rhei
                .iter()
                .map(|(id, resolved)| (id.clone(), resolved.machine.clone()))
                .collect(),
        }
    }

    /// Summarize the exact resolved sources supplied to semantic validation.
    /// The default is first; the `BTreeMap` makes remaining groups and their
    /// owners deterministic by rhei id. §FS-rhei-validate.6
    fn validation_sources(&self, rhei_ids: &[String]) -> Vec<ValidationMachineSource> {
        let mut default_rheis = rhei_ids
            .iter()
            .filter(|rhei_id| !self.per_rhei.contains_key(*rhei_id))
            .cloned()
            .collect::<Vec<_>>();
        default_rheis.sort();

        let mut sources = vec![ValidationMachineSource {
            path: self.default.path.clone(),
            project_default: true,
            rheis: default_rheis,
        }];
        for (rhei_id, resolved) in &self.per_rhei {
            if let Some(source) =
                sources.iter_mut().find(|source| source.path == resolved.path)
            {
                source.rheis.push(rhei_id.clone());
            } else {
                sources.push(ValidationMachineSource {
                    path: resolved.path.clone(),
                    project_default: false,
                    rheis: vec![rhei_id.clone()],
                });
            }
        }
        sources
    }

    /// One group per distinct complete machine, regardless of source path.
    /// The project default comes first and stays unqualified; additional
    /// groups carry the rhei ids that run them.
    // §FS-rhei-states-cmd.3: one rendered block per genuinely distinct machine.
    fn machine_groups(&self) -> Vec<MachineGroup<'_>> {
        let default_fingerprint = self.default.machine.fingerprint();
        let mut out = vec![(
            default_fingerprint.clone(),
            MachineGroup { resolved: &self.default, rheis: Vec::new() },
        )];
        for (rhei_id, resolved) in &self.per_rhei {
            let fingerprint = resolved.machine.fingerprint();
            // Identical content is already represented by the project default,
            // even from another file; keep it unqualified. §FS-rhei-states-cmd.3
            if fingerprint == default_fingerprint {
                continue;
            }
            match out.iter_mut().find(|(seen, _)| *seen == fingerprint) {
                Some((_, group)) => group.rheis.push(rhei_id.as_str()),
                None => out.push((
                    fingerprint,
                    MachineGroup { resolved, rheis: vec![rhei_id.as_str()] },
                )),
            }
        }
        out.into_iter().map(|(_, group)| group).collect()
    }

    /// [`machine_groups`](Self::machine_groups) narrowed to named rheis. Every
    /// group then names its rheis, including one that runs the project default
    /// — under narrowing "the default" is no longer a statement about the rest
    /// of the project. An empty scope is the whole project.
    // §FS-rhei-states-cmd.3: `--rhei` narrows which machines are reported.
    fn machine_groups_for_scope<'a>(&'a self, scope: &'a [String]) -> Vec<MachineGroup<'a>> {
        if scope.is_empty() {
            return self.machine_groups();
        }
        let mut out: Vec<(String, MachineGroup<'a>)> = Vec::new();
        for rhei_id in scope {
            let resolved = self.per_rhei.get(rhei_id).unwrap_or(&self.default);
            let fingerprint = resolved.machine.fingerprint();
            match out.iter_mut().find(|(seen, _)| *seen == fingerprint) {
                Some((_, group)) => group.rheis.push(rhei_id.as_str()),
                None => out.push((
                    fingerprint,
                    MachineGroup { resolved, rheis: vec![rhei_id.as_str()] },
                )),
            }
        }
        out.into_iter().map(|(_, group)| group).collect()
    }
}

/// One distinct machine and the rheis running it. §FS-rhei-states-cmd.3
struct MachineGroup<'a> {
    resolved: &'a ResolvedStateMachine,
    /// Rhei ids that resolved to this exact machine. Empty on the project
    /// default group, which governs every rhei that declares nothing.
    rheis: Vec<&'a str>,
}

/// Everything the execution paths need per rhei: the machine set for state
/// classification plus the callback base each rhei's transitions run under —
/// a self-declared machine's callbacks resolve relative to *its* states file,
/// exactly as when that workspace runs standalone.
// §DA-per-rhei-state-machines
#[derive(Clone)]
struct ExecutionMachines {
    set: rhei_validator::MachineSet,
    default_callbacks: CallbackPaths,
    per_rhei_callbacks: BTreeMap<String, CallbackPaths>,
    /// Members admitted after startup route agent runtime artifacts to their
    /// own execution root; initial members retain the project runtime.
    /// §FS-rhei-panta.6.2
    member_local_runtimes: BTreeSet<String>,
    /// The `--state-machine` this invocation resolved under, when one was
    /// given. A run records it so an attached surface renders the machine the
    /// run is executing instead of whatever the default resolves to.
    // §FS-rhei-run-headless.5
    state_machine_override: Option<PathBuf>,
}

impl ExecutionMachines {
    fn build(resolved: &ResolvedMachineSet, input: &Path, loaded: &LoadedPlan) -> MietteResult<Self> {
        let default_callbacks = resolve_callback_paths(resolved.default.path.as_deref(), input)?;
        let mut per_rhei_callbacks = BTreeMap::new();
        // Every project member needs its own plan path, even with an inherited
        // machine; the machine file still supplies the callback working dir.
        // §FS-rhei-panta.6.2 §AR-rhei-panta.5
        if loaded.is_panta_project() {
            for (rhei_id, root) in &loaded.rhei_roots {
                let machine = resolved.per_rhei.get(rhei_id).unwrap_or(&resolved.default);
                let plan = loaded.rhei_plans.get(rhei_id).unwrap_or(root);
                let plan = rhei_core::workspace::workspace_dir(plan).unwrap_or_else(|| plan.clone());
                per_rhei_callbacks.insert(
                    rhei_id.clone(),
                    resolve_callback_paths(machine.path.as_deref(), &plan)?,
                );
            }
        }
        Ok(Self {
            set: resolved.validator_set(),
            default_callbacks,
            per_rhei_callbacks,
            member_local_runtimes: BTreeSet::new(),
            state_machine_override: None,
        })
    }

    /// Remember the explicit machine override, for the run descriptor.
    // §FS-rhei-run-headless.5
    fn with_state_machine_override(mut self, path: Option<&Path>) -> Self {
        self.state_machine_override = path.map(Path::to_path_buf);
        self
    }

    fn for_task(&self, id: &TaskId) -> &rhei_validator::StateMachine {
        self.set.for_task(id)
    }

    fn for_task_str(&self, task_id: &str) -> &rhei_validator::StateMachine {
        let rhei_id = task_id.split('.').next().unwrap_or(task_id);
        self.set.per_rhei.get(rhei_id).unwrap_or(&self.set.default)
    }

    fn callbacks_for_str(&self, task_id: &str) -> &CallbackPaths {
        let rhei_id = task_id.split('.').next().unwrap_or(task_id);
        self.per_rhei_callbacks.get(rhei_id).unwrap_or(&self.default_callbacks)
    }

    fn uses_member_local_runtime(&self, task_id: &str) -> bool {
        let rhei_id = task_id.split('.').next().unwrap_or(task_id);
        self.member_local_runtimes.contains(rhei_id)
    }
}

/// Resolve every machine a loaded plan runs under: the project default, plus
/// one entry per rhei whose machine is not that default.
///
/// Per rhei, the deprecated `**States:**` pass runs first and wins wherever it
/// resolves (§FS-rhei-states-deprecation.1); where it resolves nothing, clause 1
/// reads the `states.yaml` in the rhei's own execution root whatever the index
/// says, and a rhei with no file of its own is governed by the default. An
/// explicit `--state-machine` stays a whole-scope override and errors when a
/// rhei in scope declares a different machine name.
// §FS-rhei-plan-language.1.3 §AR-rhei-panta.4
fn resolve_state_machines_for_loaded_plan(
    input: &Path,
    loaded: &LoadedPlan,
    state_machine_path: Option<&Path>,
) -> MietteResult<ResolvedMachineSet> {
    let default = resolve_state_machine_for_loaded_plan(input, loaded, state_machine_path)?;

    if let Some(override_path) = state_machine_path {
        reject_declarations_the_override_cannot_mean(loaded, override_path, &default)?;
        return Ok(ResolvedMachineSet { default, per_rhei: BTreeMap::new() });
    }

    // Iterating the roots rather than the declarations is the whole of the
    // change: a rhei that declares nothing used to have no entry here, which
    // is why its own root was never consulted. §FS-rhei-plan-language.1.3
    let mut per_rhei = BTreeMap::new();
    if loaded.is_panta_project() {
        let mut roots: Vec<(&String, &PathBuf)> = loaded.rhei_roots.iter().collect();
        roots.sort();
        for (rhei_id, root) in roots {
            let declared = loaded.rhei_machines.get(rhei_id).map(String::as_str);
            if let Some(resolved) =
                resolve_rhei_machine(input, loaded, rhei_id, root, declared, &default)?
            {
                per_rhei.insert(rhei_id.clone(), resolved);
            }
        }
    }

    Ok(ResolvedMachineSet { default, per_rhei })
}

/// `--state-machine` replaces resolution for the whole scope, so a rhei in
/// scope that declares some other machine is a contradiction rather than a
/// narrowing. §AR-rhei-panta.4
fn reject_declarations_the_override_cannot_mean(
    loaded: &LoadedPlan,
    override_path: &Path,
    default: &ResolvedStateMachine,
) -> MietteResult<()> {
    let mut declared: Vec<(&String, &String)> = loaded.rhei_machines.iter().collect();
    declared.sort();
    for (rhei_id, machine_name) in declared {
        if *machine_name == default.machine.name {
            continue;
        }
        return Err(miette!(
            help = "--state-machine replaces resolution for the whole scope. Narrow the scope with --rhei, or drop the override and let each rhei resolve its own machine.",
            "--state-machine '{}' declares '{}', but rhei '{rhei_id}' declares state \
             machine '{machine_name}'. The override replaces resolution for the whole \
             scope; it cannot reinterpret that rhei's states under another machine. \
             Narrow the invocation or drop the override.",
            override_path.display(),
            default.machine.name,
        ));
    }
    Ok(())
}

/// One rhei's machine, or `None` where the project default governs it.
///
/// The deprecated declaration pass first (§FS-rhei-states-deprecation.1), then
/// clause 1 — the `states.yaml` in this rhei's own execution root, whatever
/// its `name:` and whatever the index says. A declaration neither of them
/// resolved is still the validation error, never a fall through to the
/// project default. §FS-rhei-plan-language.1.3
fn resolve_rhei_machine(
    input: &Path,
    loaded: &LoadedPlan,
    rhei_id: &str,
    root: &Path,
    declared: Option<&str>,
    default: &ResolvedStateMachine,
) -> MietteResult<Option<ResolvedStateMachine>> {
    // The basin is synthetic: it has no index of its own, holds unfiled
    // tickets, and runs under the project default. §FS-rhei-panta.2
    if rhei_id == workspace::BASIN_RHEI_ID {
        return Ok(None);
    }

    let subject = DeclarationSubject::Rhei { id: rhei_id, root };
    if let Some(machine_name) = declared {
        if let Some(found) =
            resolve_declared_rhei_machine(input, loaded, rhei_id, machine_name, default)?
        {
            warn_about_deprecated_resolution(&subject, machine_name, &found);
            return Ok(Some(found.resolved));
        }
    }

    let candidate = root.join("states.yaml");
    if candidate.is_file() {
        let machine = load_state_machine(Some(&candidate))?;
        if let Some(machine_name) = declared {
            warn_declaration_nothing_supplies(&subject, machine_name, &candidate, &machine.name);
        }
        return Ok(Some(ResolvedStateMachine { machine, path: Some(candidate) }));
    }

    match declared {
        // A declaration naming a machine nothing supplies never silently
        // resolves to some other machine. §FS-rhei-plan-language.1.3
        Some(machine_name) => Err(miette!(
            help = missing_state_machine_help(),
            "rhei '{rhei_id}' declares state machine '{machine_name}', but no states file \
             declaring it was found in the rhei's root, the project root, or any other \
             rhei root. This project declares: {}. Add a `states.yaml` declaring \
             '{machine_name}' next to the rhei, or pass --state-machine <path>.",
            state_machine_names_in(input, loaded).join(", "),
        )),
        None => Ok(None),
    }
}

/// Every rhei execution root in the project, deduplicated and ordered.
// §AR-rhei-panta.4
fn sorted_rhei_roots(loaded: &LoadedPlan) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = loaded.rhei_roots.values().cloned().collect();
    roots.sort();
    roots.dedup();
    roots
}

/// The same for the roots a ticket resolves through, which is what the
/// project default's cross-root match has always searched.
// §AR-rhei-panta.4
fn sorted_task_roots(loaded: &LoadedPlan) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = loaded.task_roots.values().cloned().collect();
    roots.sort();
    roots.dedup();
    roots
}

/// Every state machine name a `**States:**` declaration in this project can
/// resolve to: the name inside each `states.yaml` the declaration rules reach,
/// plus the built-in default that a rhei declaring nothing runs under.
///
/// A wrong value for a flag with a declared set of legal values lists that set
/// everywhere else in the CLI; the set for `--states` simply lives in files.
// §AR-rhei-panta.4 §FS-rhei-new.1.2
fn state_machine_names_in(input: &Path, loaded: &LoadedPlan) -> Vec<String> {
    let mut names: BTreeSet<String> =
        BTreeSet::from([rhei_validator::StateMachine::builtin_default().name]);
    let mut candidates = vec![auto_state_machine_path(input)];
    candidates.extend(sorted_rhei_roots(loaded).into_iter().map(|root| root.join("states.yaml")));
    for candidate in candidates {
        if candidate.is_file() {
            if let Ok(machine) = load_state_machine(Some(&candidate)) {
                names.insert(machine.name);
            }
        }
    }
    names.into_iter().collect()
}

/// The same set, for a completion that has only a plan path to work from.
// §AR-rhei-panta.4
fn discoverable_state_machine_names(plan: Option<&Path>) -> Vec<String> {
    let Some(plan) = plan else {
        return vec![rhei_validator::StateMachine::builtin_default().name];
    };
    match load_plan_leniently(plan) {
        Ok(loaded) => state_machine_names_in(plan, &loaded),
        Err(_) => vec![rhei_validator::StateMachine::builtin_default().name],
    }
}

fn auto_state_machine_path(input: &Path) -> PathBuf {
    if let Some(project_dir) = workspace::panta_project_dir(input) {
        project_dir.join("states.yaml")
    } else if workspace::is_workspace(input) {
        input.join("states.yaml")
    } else {
        input.parent().unwrap_or_else(|| Path::new(".")).join("states.yaml")
    }
}

/// If `input` references a Directory Workspace via its inner `index.rhei.md`
/// file, return the workspace root directory; otherwise return `input`
/// unchanged. This lets command handlers continue to use the existing
/// `workspace::is_workspace(input)` + `input.join(...)` pattern regardless of
/// which form the user supplied on the command line.
fn normalize_workspace_input(input: &Path) -> PathBuf {
    workspace::panta_project_dir(input)
        .or_else(|| workspace::workspace_dir(input))
        .unwrap_or_else(|| input.to_path_buf())
}

/// The project default: what governs the Panta root, the basin, and every
/// rhei with no machine of its own.
///
/// The manifest's own deprecated declaration resolves first and wins wherever
/// it resolves (§FS-rhei-states-deprecation.1); behind it the `states.yaml` at
/// the project root is the default by its presence, whether or not
/// `index.panta.md` names it, and the built-in machine is the last word. A
/// declaration neither pass supplied is still a validation error.
// §FS-rhei-plan-language.1.3
fn resolve_state_machine_for_loaded_plan(
    input: &Path,
    loaded: &LoadedPlan,
    state_machine_path: Option<&Path>,
) -> MietteResult<ResolvedStateMachine> {
    if let Some(path) = state_machine_path {
        let machine = load_state_machine(Some(path))?;
        if loaded.rhei.states_declared && machine.name != loaded.rhei.states.trim() {
            return Err(miette!(
                help = states_declaration_help(),
                "plan declares state machine '{}', but --state-machine '{}' declares '{}'",
                loaded.rhei.states.trim(),
                path.display(),
                machine.name
            ));
        }
        return Ok(ResolvedStateMachine { machine, path: Some(path.to_path_buf()) });
    }

    let candidate = auto_state_machine_path(input);
    let declared =
        loaded.rhei.states_declared.then(|| loaded.rhei.states.trim()).filter(|name| !name.is_empty());
    let root = candidate.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let declaring = declaring_plan_file(input);
    let subject = DeclarationSubject::Manifest { root: &root, file: &declaring };

    if let Some(declared_name) = declared {
        if let Some(found) = resolve_declared_project_default(input, loaded, declared_name)? {
            warn_about_deprecated_resolution(&subject, declared_name, &found);
            return Ok(found.resolved);
        }
    }

    // Clauses 1 and 2 name the same file here: a project's own execution root
    // is the project root. §FS-rhei-plan-language.1.3
    if candidate.is_file() {
        let machine = load_state_machine(Some(&candidate))?;
        if let Some(declared_name) = declared {
            warn_declaration_nothing_supplies(&subject, declared_name, &candidate, &machine.name);
        }
        return Ok(ResolvedStateMachine { machine, path: Some(candidate) });
    }

    if let Some(declared_name) = declared {
        return Err(miette!(
            help = states_declaration_help(),
            "plan declares state machine '{declared_name}', but no auto-discovered states file \
             was found at '{}' or, by name, in any rhei root.\nUse --state-machine <path> to \
             override the default location.",
            candidate.display()
        ));
    }

    Ok(ResolvedStateMachine { machine: rhei_validator::StateMachine::builtin_default(), path: None })
}

/// Resolve the state machine for `rhei states`.
///
/// An explicit `--state-machine` answers on its own — the command must stay
/// usable for inspecting a machine file anywhere. Otherwise the target plan
/// decides, exactly as it does for `validate`, `list`, and `run`.
///
/// Discovery is best-effort in both directions that matter: outside any
/// project there is nothing to resolve against, so the built-in default is the
/// honest answer; and an auto-discovered plan that fails to load must not make
/// `rhei states` unusable while the author is repairing that very plan. An
/// explicitly named plan is strict, because the user asked about *that* plan.
///
/// Returns the resolved machines and the rhei ids the invocation is narrowed
/// to — from `--rhei`, or from a target that named one member rhei. Empty is
/// the whole project.
// §FS-rhei-panta.6: project-wide by default, narrowed by `--rhei`.
fn resolve_state_machine_for_states_command(
    input: Option<PathBuf>,
    state_machine: Option<&Path>,
    selected_rheis: &[String],
) -> MietteResult<(ResolvedMachineSet, Vec<String>)> {
    if let Some(path) = state_machine {
        let machine = load_state_machine(Some(path))?;
        return Ok((
            ResolvedMachineSet::single(ResolvedStateMachine {
                machine,
                path: Some(path.to_path_buf()),
            }),
            Vec::new(),
        ));
    }

    let explicit = input.is_some();
    let builtin = || {
        (
            ResolvedMachineSet::single(ResolvedStateMachine {
                machine: rhei_validator::StateMachine::builtin_default(),
                path: None,
            }),
            Vec::new(),
        )
    };
    let target = match resolve_plan_target(input) {
        Ok(target) => target,
        // No plan and no project to speak of: the built-in default is what any
        // plan authored here would get.
        Err(_) if !explicit => return Ok(builtin()),
        Err(err) => return Err(err),
    };

    let selected = target.scope_with(selected_rheis);
    let target = target.path;
    match load_plan(&target).and_then(|loaded| {
        // Validate the selection against the project before reporting on it,
        // so an unknown id names the available rheis instead of quietly
        // narrowing to nothing. §FS-rhei-panta.6
        let scope = resolve_rhei_scope(&loaded, &selected)?;
        let resolved = resolve_state_machines_for_loaded_plan(&target, &loaded, None)?;
        Ok((resolved, scope.map(|ids| ids.into_iter().collect()).unwrap_or_default()))
    }) {
        Ok(resolved) => Ok(resolved),
        Err(err) if !explicit && selected_rheis.is_empty() => {
            eprintln!(
                "warning: could not resolve the state machine from {} ({err}); showing the \
                 built-in default",
                target.display()
            );
            Ok(builtin())
        }
        Err(err) => Err(err),
    }
}

/// Human-readable label for the state machine source, used in diagnostics.
fn state_machine_label(path: Option<&Path>) -> String {
    match path {
        Some(p) => format!("'{}'", p.display()),
        None => "the built-in default state machine".to_string(),
    }
}

/// Execute the `states` subcommand: resolve the state machine the plan or
/// project actually runs under, then print its states and transitions.
/// Resolution matches every other command. §FS-rhei-plan-language.1.3
fn states_command(
    input: Option<PathBuf>,
    state_machine: Option<&Path>,
    selected_rheis: &[String],
    as_json: bool,
) -> MietteResult<()> {
    let (resolved, scope) =
        resolve_state_machine_for_states_command(input, state_machine, selected_rheis)?;
    let groups = resolved.machine_groups_for_scope(&scope);

    if as_json {
        // JSON keeps its stable single-object shape when one machine governs;
        // a heterogeneous project renders as an array, default first.
        // §FS-rhei-states-cmd.5
        let rendered = if groups.len() == 1 {
            render_state_machine_json(&groups[0].resolved.machine).map_err(|err| {
                miette!(help = internal_error_help(), "failed to serialize state machine: {err}")
            })?
        } else {
            let values = groups
                .iter()
                .map(|group| render_state_machine_json_value(&group.resolved.machine))
                .collect::<Vec<_>>();
            serde_json::to_string_pretty(&values).map_err(|err| {
                miette!(help = internal_error_help(), "failed to serialize state machines: {err}")
            })?
        };
        println!("{rendered}");
        return Ok(());
    }

    // Text: one block per distinct machine, the project default first. A block
    // names the rheis running it, except the project-wide default block, whose
    // reach is "everything that declares nothing". §FS-rhei-states-cmd.3
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            println!();
        }
        let label = state_machine_label(group.resolved.path.as_deref());
        if group.rheis.is_empty() {
            println!("Source: {label}");
        } else {
            println!("Source: {label} (rhei: {})", group.rheis.join(", "));
        }
        println!("{}", render_state_machine_text(&group.resolved.machine));
    }

    Ok(())
}
