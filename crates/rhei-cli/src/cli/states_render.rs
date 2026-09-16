/// When a supervising state's task is woken, in the words of its
/// `execute_on:` value.
///
/// The value is a scope and an event, and a reader of `rhei states` is asking
/// one question of it: which moves under this task bring it back. The bare
/// value answers that only to someone who already knows the grammar.
// §FS-rhei-supervision.1.1 §FS-rhei-states-cmd.4
fn executes_on_phrase(execute_on: rhei_validator::ExecuteOn) -> &'static str {
    match execute_on {
        rhei_validator::ExecuteOn::ChildTerminal => "every finished child",
        rhei_validator::ExecuteOn::ChildTransition => "every child transition",
        rhei_validator::ExecuteOn::DescendantTerminal => "every finished descendant",
        rhei_validator::ExecuteOn::DescendantTransition => {
            "every descendant transition \u{2014} one invocation per hop"
        }
    }
}

fn render_state_machine_text(machine: &rhei_validator::StateMachine) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "State machine: {} (version: {})\n",
        machine.name,
        format_version(&machine.version)
    ));
    if !machine.models.is_empty() {
        out.push_str(&format!("Models: {}\n", machine.models.join(", ")));
    }
    if !machine.prompt_templates.is_empty() {
        out.push_str(&format!(
            "Prompt templates: {}\n",
            machine.prompt_templates.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if let Some(profiles) = machine.profiles.as_ref() {
        out.push_str("Profiles:\n");
        for (name, profile) in profiles {
            out.push_str(&format!(
                "  {name}: initial={}, allowed=[{}]\n",
                profile.initial,
                profile.allowed.join(", ")
            ));
        }
    }
    if let Some(policy) = machine.node_policy.as_ref() {
        out.push_str(&format!(
            "Node policy: root={}, default={}\n",
            policy.root, policy.default
        ));
    }

    out.push_str("\nStates:\n");
    if machine.states.is_empty() {
        out.push_str("  (none defined)\n");
    } else {
        for (idx, (name, def)) in machine.states.iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            let mut flags = Vec::new();
            if def.terminal {
                flags.push("final");
            }
            if def.gating {
                flags.push("gating");
            }
            if def.concurrent {
                flags.push("concurrent");
            }
            let flag_suffix =
                if flags.is_empty() { String::new() } else { format!(" [{}]", flags.join(", ")) };
            let description = def.description.as_deref().unwrap_or("");
            out.push_str(&format!("  {name}{flag_suffix}"));
            if !description.is_empty() {
                out.push_str(&format!(" — {description}"));
            }
            out.push('\n');
            if let Some(visits) = def.visits {
                out.push_str(&format!("      Visits: {visits}\n"));
            }
            // §FS-rhei-supervision.1.1: when the supervisor wakes, in the
            // words of the value — the bare value read as a noun ("task") said
            // nothing about which events reach it.
            if let Some(execute_on) = def.execute_on() {
                out.push_str(&format!("      Executes on: {}\n", executes_on_phrase(execute_on)));
            }
            if let Some(poll) = def.poll.as_ref() {
                // §FS-rhei-states-cmd.4: appended only when authored, so a
                // machine-backoff poll prints the line it printed before.
                let waits_on = poll
                    .waiting_on()
                    .map(|label| format!(", waiting_on={label}"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "      Poll: interval={}, max_attempts={}{waits_on}\n",
                    poll.interval, poll.max_attempts
                ));
            }
            if let Some(target) = def.target.as_deref() {
                out.push_str(&format!("      Target: {target}\n"));
            }
            if !def.all_targets.is_empty() {
                out.push_str(&format!("      Targets: {}\n", def.all_targets.join(", ")));
            }
            if !def.all_models.is_empty() {
                out.push_str(&format!("      Models: {}\n", def.all_models.join(", ")));
            } else if let Some(model) = def.model.as_deref() {
                out.push_str(&format!("      Model: {model}\n"));
            }
            if let Some(agent) = def.agent.as_ref() {
                out.push_str(&format!("      Agent: {}\n", agent.id()));
            }
            if let Some(mode) = def.agent_mode.as_deref() {
                out.push_str(&format!("      Agent mode: {mode}\n"));
            }
            // Omission preserves the established text shape. §FS-rhei-states-cmd.4
            if let Some(effort) = def.effort {
                out.push_str(&format!("      Effort: {}\n", effort.as_str()));
            }
            if let Some(timeout) = def.agent_timeout.as_deref() {
                out.push_str(&format!("      Agent timeout: {timeout}\n"));
            }
            if def.program.is_some() {
                out.push_str("      Program: configured\n");
            }
            if let Some(timeout) = def.program_timeout.as_deref() {
                out.push_str(&format!("      Program timeout: {timeout}\n"));
            }
            if let Some(reference) = def.prompt_template.as_ref() {
                out.push_str(&format!("      Prompt template: {}\n", reference.name()));
            }
            if let Some(mcp_servers) = def.mcp_servers.as_ref() {
                let ids = mcp_servers.iter().map(|entry| entry.id()).collect::<Vec<_>>();
                out.push_str(&format!("      MCP servers: {}\n", ids.join(", ")));
            }
            if let Some(skills) = def.skills.as_ref() {
                let ids = skills.iter().map(|entry| entry.id()).collect::<Vec<_>>();
                out.push_str(&format!("      Skills: {}\n", ids.join(", ")));
            }
            if def.snapshot.is_some() {
                out.push_str("      Snapshot: configured\n");
            }
            if !def.inputs.is_empty() {
                out.push_str("      Inputs:\n");
                for artifact in &def.inputs {
                    out.push_str(&format!("        - {}: {}\n", artifact.name, artifact.path));
                }
            }
            if !def.outputs.is_empty() {
                out.push_str("      Outputs:\n");
                for artifact in &def.outputs {
                    out.push_str(&format!("        - {}: {}\n", artifact.name, artifact.path));
                }
            }
            if let Some(personality) =
                def.personality.as_deref().map(str::trim).filter(|s| !s.is_empty())
            {
                out.push_str(&format!("      Personality: {personality}\n"));
            }
            if let Some(instructions) = def.instructions.as_deref() {
                for line in instructions.lines() {
                    out.push_str(&format!("      {line}\n"));
                }
            }
        }
    }

    out.push_str("\nTransitions:\n");
    if machine.transitions.is_empty() {
        out.push_str("  (none declared)\n");
    } else {
        for rule in &machine.transitions {
            out.push_str(&format!("  {} -> {}", rule.from.0, rule.to.0));
            let mut annotations = Vec::new();
            if let Some(cb) = rule.on_leave.as_ref() {
                annotations.push(format!("on_leave={}", cb.0));
            }
            if let Some(cb) = rule.on_enter.as_ref() {
                annotations.push(format!("on_enter={}", cb.0));
            }
            if let Some(cond) = rule.condition.as_ref() {
                annotations.push(format!("when={cond}"));
            }
            if let Some(t) = rule.timeout.as_ref() {
                annotations.push(format!("timeout={t}"));
            }
            if !annotations.is_empty() {
                out.push_str(&format!(" ({})", annotations.join(", ")));
            }
            out.push('\n');
        }
    }

    out
}

fn render_state_machine_json(machine: &rhei_validator::StateMachine) -> Result<String> {
    let states: Vec<serde_json::Value> = machine
        .states
        .iter()
        .map(|(name, def)| {
            let mut rendered = serde_json::json!({
                "name": name,
                "description": &def.description,
                "prompt_template": &def.prompt_template,
                "instructions": &def.instructions,
                "personality": &def.personality,
                "final": def.terminal,
                "gating": def.gating,
                "concurrent": def.concurrent,
                "poll": &def.poll,
                "visits": def.visits,
                // Beside `visits` on purpose: one bounds entries, the other
                // spawns within one entry. §FS-rhei-agents.3.2.3
                "attempts": def.attempts,
                "execute_on": &def.execute_on,
                "target": &def.target,
                "all_targets": &def.all_targets,
                "all_models": &def.all_models,
                "model": &def.model,
                "agent": def.agent.as_ref().map(|agent| agent.id()),
                "agent_mode": &def.agent_mode,
                "agent_timeout": &def.agent_timeout,
                "program": &def.program,
                "program_timeout": &def.program_timeout,
                "mcp_servers": &def.mcp_servers,
                "skills": &def.skills,
                "snapshot": &def.snapshot,
                "inputs": &def.inputs,
                "outputs": &def.outputs,
            });
            // Authored effort is inspectable, but an omitted field is not
            // materialized as null. §FS-rhei-states-cmd.5
            if let Some(effort) = def.effort {
                rendered
                    .as_object_mut()
                    .expect("state JSON is an object")
                    .insert("effort".to_string(), serde_json::json!(effort.as_str()));
            }
            rendered
        })
        .collect();

    let transitions =
        serde_json::to_value(&machine.transitions).context("serialize transitions")?;
    let version =
        serde_json::to_value(&machine.version).context("serialize state machine version")?;

    let payload = serde_json::json!({
        "name": machine.name,
        "models": &machine.models,
        "prompt_templates": &machine.prompt_templates,
        "profiles": &machine.profiles,
        "node_policy": &machine.node_policy,
        "version": version,
        "states": states,
        "transitions": transitions,
    });

    serde_json::to_string_pretty(&payload).context("render state machine as JSON")
}

/// The same payload as [`render_state_machine_json`], unserialized — for the
/// heterogeneous-project array shape. §FS-rhei-states-cmd.5
fn render_state_machine_json_value(machine: &rhei_validator::StateMachine) -> serde_json::Value {
    match render_state_machine_json(machine)
        .ok()
        .and_then(|rendered| serde_json::from_str(&rendered).ok())
    {
        Some(value) => value,
        None => serde_json::json!({ "name": machine.name }),
    }
}

fn format_version(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        other => serde_yaml::to_string(other)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
    }
}

/// Read the markdown plan source file from disk.
///
/// Through [`read_plan_source`], because a command may be holding the file's
/// own lock while it asks the loader to read it: on Windows that lock refuses
/// a second open, including this process's.
// §FS-rhei-new.4
fn read_input_file(path: &Path) -> MietteResult<String> {
    read_plan_source(path, "failed to read input file")
}

/// A loaded plan with optional workspace task-to-file mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadedPlanKind {
    SingleFile,
    Workspace,
    PantaProject,
}

/// Name any member rhei carrying its own settings file, under either of rhei's
/// homes: settings resolve once, at the project root, so a copy inside a member
/// is read by nothing and its agent registry silently vanished.
/// §FS-rhei-agents.1.1
fn ignored_member_settings_warnings(input: &Path, loaded: &LoadedPlan) -> Vec<String> {
    if !loaded.is_panta_project() {
        return Vec::new();
    }
    let Some(project) = workspace::panta_project_dir(input) else {
        return Vec::new();
    };
    let Ok(entries) = workspace::discover_rhei_entries(&project) else {
        return Vec::new();
    };
    entries
        .into_iter()
        // A member's own file counts under either home, but the move it names
        // is always the current one. §FS-rhei-templates.1.1
        .filter_map(|entry| resolve_rhei_home_file(&entry, PROJECT_SETTINGS_FILE))
        .map(|settings| {
            format!(
                "{} is ignored: settings resolve at the project root, so move its contents into \
                 {} — nothing reads a member rhei's own settings file",
                settings.path().display(),
                rhei_home_write_path(&project, PROJECT_SETTINGS_FILE).display()
            )
        })
        .collect()
}

/// One warning per rhei that loaded but holds no tickets. Empty is valid, but a
/// mistyped `tasks/` loads identically, so name it rather than report a bare
/// green. §FS-rhei-plan-language.1.2
fn empty_rhei_warnings(loaded: &LoadedPlan) -> Vec<String> {
    match loaded.kind {
        LoadedPlanKind::PantaProject => loaded
            .rhei_ids
            .iter()
            .filter(|id| {
                let prefix = format!("{id}.");
                !loaded.rhei.tasks.iter().any(|task| task.id.to_string().starts_with(&prefix))
            })
            .map(|id| empty_rhei_help(id))
            .collect(),
        LoadedPlanKind::Workspace if loaded.rhei.tasks.is_empty() => vec![
            "this workspace holds no tickets: task files are the non-hidden `*.md` files \
             under `tasks/`"
                .to_string(),
        ],
        // A single-file rhei may be empty too, and an emptied one looks exactly
        // like a freshly created one. §FS-rhei-plan-language.1.1
        LoadedPlanKind::SingleFile if loaded.rhei.tasks.is_empty() => vec![
            "this rhei holds no tickets: its tickets are the task nodes under its \
             `## Tasks` section"
                .to_string(),
        ],
        _ => Vec::new(),
    }
}

struct LoadedPlan {
    rhei: rhei_core::ast::Rhei,
    kind: LoadedPlanKind,
    /// For directory workspaces: maps task ID string → source file path.
    /// For Panta projects: maps project-qualified task IDs to owning files.
    /// Empty for single-file plans.
    task_sources: HashMap<String, PathBuf>,
    /// For Panta projects: maps project-qualified task IDs to owning rhei roots.
    task_roots: HashMap<String, PathBuf>,
    /// For Panta projects: link bases for merged content sections, in section order.
    content_section_roots: Vec<PathBuf>,
    /// For Panta projects: rhei ids in load order (`basin` last when present).
    rhei_ids: Vec<String>,
    /// Machine name each rhei declared with its own `**States:**`, when it
    /// did. §DA-per-rhei-state-machines
    rhei_machines: HashMap<String, String>,
    /// Execution root of each rhei, keyed by rhei id. §AR-rhei-panta.4
    rhei_roots: HashMap<String, PathBuf>,
    /// Title of each rhei, keyed by rhei id. §FS-rhei-memory.3.1
    rhei_titles: HashMap<String, String>,
    /// Plan document of each rhei, keyed by rhei id. §FS-rhei-memory.3.4
    rhei_plans: HashMap<String, PathBuf>,
    /// Rheis a lenient load skipped, one message each; empty for a strict load.
    unloadable: Vec<String>,
}

impl LoadedPlan {
    /// Return the file path that contains the given task.
    /// For single-file plans, returns `fallback` (the plan file itself).
    fn task_file(&self, task_id: &str, fallback: &Path) -> PathBuf {
        self.task_sources.get(task_id).cloned().unwrap_or_else(|| fallback.to_path_buf())
    }

    fn task_root(&self, task_id: &str, fallback: &Path) -> PathBuf {
        self.task_roots.get(task_id).cloned().unwrap_or_else(|| fallback.to_path_buf())
    }

    fn is_panta_project(&self) -> bool {
        self.kind == LoadedPlanKind::PantaProject
    }

    /// Resolve where a merged-graph ticket's rewrites land: heading file,
    /// metadata file, in-file id, and the owning rhei's execution root.
    /// §FS-rhei-panta.6.1 routes every rewrite to the owning rhei.
    fn task_route(&self, task_id: &str, input: &Path) -> TaskRoute {
        let task_file = self.task_file(task_id, input);
        let fallback_root = match self.kind {
            LoadedPlanKind::SingleFile => {
                input.parent().unwrap_or(Path::new(".")).to_path_buf()
            }
            _ => input.to_path_buf(),
        };
        let execution_root = self.task_root(task_id, &fallback_root);
        // Ticket headings inside the owning file are rhei-local: strip the
        // project-qualifying rhei id segment. §AR-rhei-panta.3
        let local_id = rhei_local_id_str(task_id).to_string();
        // A workspace-shaped rhei keeps ticket metadata in its own index, a
        // single-file rhei in the rhei file itself. The basin has no authored
        // index, so its metadata lands in the manifest. §FS-rhei-panta.6.1
        if self.is_basin_task(task_id) {
            // `input` may name the project by directory or by manifest file;
            // resolve it rather than assuming, since not every caller
            // normalizes before routing.
            let project_dir = workspace::panta_project_dir(input)
                .unwrap_or_else(|| input.to_path_buf());
            return TaskRoute {
                task_file,
                metadata_file: project_dir.join(workspace::PANTA_INDEX_FILE),
                local_id,
                metadata_id: task_id.to_string(),
                execution_root,
            };
        }
        let metadata_file = if workspace::is_workspace(&execution_root) {
            execution_root.join("index.rhei.md")
        } else {
            task_file.clone()
        };
        let metadata_id = local_id.clone();
        TaskRoute { task_file, metadata_file, local_id, metadata_id, execution_root }
    }

    /// True when `task_id` belongs to the synthetic basin rhei of a project.
    fn is_basin_task(&self, task_id: &str) -> bool {
        self.is_panta_project()
            && task_id.split_once('.').map(|(rhei, _)| rhei) == Some(workspace::BASIN_RHEI_ID)
    }
}

/// Strip the project-qualifying rhei segment from a ticket id string. Mirrors
/// [`LoadedPlan::task_route`] heading routing; an unqualified id passes
/// through unchanged. §AR-rhei-panta.3
fn rhei_local_id_str(task_id: &str) -> &str {
    task_id.split_once('.').map(|(_, rest)| rest).unwrap_or(task_id)
}

/// True when `value` has the shape of a ticket id (`3`, `auth.1`, `auth.1.2`)
/// rather than a filesystem path: dot-separated segments, no separators, and
/// no markdown extension.
fn looks_like_task_id(value: &Path) -> bool {
    let Some(text) = value.to_str() else {
        return false;
    };
    if text.is_empty() || text.contains('/') || text.contains('\\') || text.ends_with(".md") {
        return false;
    }
    text.split('.').all(|segment| {
        !segment.is_empty()
            && segment.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

/// Resolve an omitted plan target: walk up from the current directory to the
/// nearest project (`index.panta.md`) or workspace rhei (`index.rhei.md`); a
/// lone rhei resolves in the current directory only. §FS-rhei-panta.6
fn resolve_plan_target(input: Option<PathBuf>) -> MietteResult<PlanTarget> {
    let resolved = resolve_plan_path(input)?;
    // A rhei inside a project is loaded through the project and narrowed to
    // itself, so cross-rhei priors resolve. §FS-rhei-panta.6
    match workspace::panta_member(&resolved) {
        Some((project, id)) => Ok(PlanTarget { path: project, implied_scope: vec![id] }),
        None => Ok(PlanTarget::whole(resolved)),
    }
}

fn resolve_plan_path(input: Option<PathBuf>) -> MietteResult<PathBuf> {
    if let Some(input) = input {
        // The positional slot takes a plan, but it sits where a ticket id
        // looks like it belongs — and commands that select a ticket take it
        // through `--task`. Name that mistake instead of failing later with a
        // path error about something the user never meant as a path.
        if !input.exists() {
            if looks_like_task_id(&input) {
                return Err(miette!(
                    help = "this argument takes a plan or project path, not a ticket id.",
                    "'{}' is not a path. This argument takes a plan or project; select a \
                     ticket with `--task {}` and let the plan resolve on its own.",
                    input.display(),
                    input.display()
                ));
            }
            return Err(miette!(
                help = io_error_help(&input, std::io::ErrorKind::NotFound),
                "no plan or project at '{}'. Pass a `.rhei.md` file, a workspace \
                 directory, or omit the argument to use the enclosing project.",
                input.display()
            ));
        }
        return Ok(input);
    }
    let cwd = std::env::current_dir()
        .map_err(|err| miette!(
            help = cwd_help(),
            "failed to read the current directory: {err}"
        ))?;
    let mut dir = Some(cwd.as_path());
    while let Some(current) = dir {
        if current.join(workspace::PANTA_INDEX_FILE).is_file() {
            return Ok(current.to_path_buf());
        }
        // §FS-rhei-panta.6: the conventional `panta/` child `rhei init`
        // creates resolves from anywhere in the host repository.
        let conventional = current.join("panta");
        if conventional.join(workspace::PANTA_INDEX_FILE).is_file() {
            return Ok(conventional);
        }
        if current.join("index.rhei.md").is_file() {
            return Ok(current.to_path_buf());
        }
        // §FS-rhei-panta.6: a loose rhei — counted exactly as project
        // discovery counts it — resolves in the invocation directory only;
        // ancestors are adopted solely through explicit manifests.
        if current == cwd {
            let mut plans = workspace::discover_rhei_entries(current).unwrap_or_default();
            match plans.len() {
                0 => {}
                1 => return Ok(plans.remove(0)),
                _ => {
                    let names: Vec<String> = plans
                        .iter()
                        .filter_map(|path| path.file_name())
                        .map(|name| name.to_string_lossy().into_owned())
                        .collect();
                    return Err(miette!(
help = "run `rhei init --here` to make the directory a project, or pass one of the rheis above.",

                        "{} holds {} rheis ({}) but no `index.panta.md`, so there is no \
                         single plan to pick. Pass one explicitly, or run `rhei init` to \
                         make the directory a project (writes index.panta.md)",
                        current.display(),
                        names.len(),
                        names.join(", ")
                    ));
                }
            }
        }
        dir = current.parent();
    }
    // Resolution only walks up, so a project adopted in a subdirectory is
    // invisible from the repo root — the one place bare commands get run.
    // §FS-rhei-panta.6
    let nearby = nearby_projects(&cwd);
    if !nearby.is_empty() {
        let listed = nearby
            .iter()
            .map(|path| format!("  rhei <command> {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(miette!(
            help = "run the command against one of the paths listed above, or cd into it.",
            "no Rhei plan found at or above {}. Target resolution only walks up, but there \
             {} below it:\n{}",
            cwd.display(),
            if nearby.len() == 1 { "is a project" } else { "are projects" },
            listed
        ));
    }
    Err(miette!(
help = "pass a plan path, or run `rhei init` to make this directory a project.",

        "no Rhei plan found: neither {} nor any parent directory contains an \
         `index.panta.md` project manifest, a workspace `index.rhei.md`, or a \
         `*.rhei.md` plan file. Pass a plan path (`rhei <command> path/to/plan.rhei.md`) \
         or run inside a project",
        cwd.display()
    ))
}

/// Panta projects within a few levels below `root`, for a resolution failure
/// that would otherwise just say "not found". Bounded in depth and breadth so
/// the search stays cheap, and skipping hidden and build directories.
fn nearby_projects(root: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 3;
    const MAX_HITS: usize = 5;
    let mut found = Vec::new();
    let mut frontier = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = frontier.pop() {
        if depth >= MAX_DEPTH || found.len() >= MAX_HITS {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter(|path| {
                path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
                    !name.starts_with('.') && !matches!(name, "target" | "node_modules")
                })
            })
            .collect();
        children.sort();
        for child in children {
            if workspace::is_panta_project(&child) {
                found.push(child);
                if found.len() >= MAX_HITS {
                    break;
                }
            } else {
                frontier.push((child, depth + 1));
            }
        }
    }
    found.sort();
    found
}

/// A resolved plan target: the document to load, plus the rhei ids the
/// invocation is implicitly narrowed to.
///
/// `implied_scope` is non-empty only when the caller pointed at a rhei that
/// belongs to a Panta project. The project is what gets loaded — a member rhei
/// cannot resolve its cross-rhei `**Prior:**` or its state machine without it —
/// and the id it was pointed at narrows the tickets acted on, exactly as
/// `--rhei <id>` would.
// §FS-rhei-panta.6: a rhei that belongs to a project always loads through it.
#[derive(Debug, Clone)]
struct PlanTarget {
    path: PathBuf,
    implied_scope: Vec<String>,
}

impl PlanTarget {
    fn whole(path: PathBuf) -> Self {
        Self { path, implied_scope: Vec::new() }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// The `--rhei` selection this invocation should use: the flag's value when
    /// given, otherwise the rhei the target pointed at.
    fn scope_with(&self, selected: &[String]) -> Vec<String> {
        if selected.is_empty() {
            self.implied_scope.clone()
        } else {
            selected.to_vec()
        }
    }
}

/// The set of rhei ids a project-scoped invocation is narrowed to. `None` is
/// the whole project. §FS-rhei-panta.6
type RheiScope = Option<BTreeSet<String>>;

/// Validate a `--rhei` selection against the loaded project and resolve it into
/// a scope. An unknown id is an error that names the available rheis; an empty
/// selection leaves the invocation project-wide. §FS-rhei-panta.6
fn resolve_rhei_scope(loaded: &LoadedPlan, selected: &[String]) -> MietteResult<RheiScope> {
    if selected.is_empty() {
        return Ok(None);
    }
    let available: BTreeSet<&str> = loaded.rhei_ids.iter().map(String::as_str).collect();
    let mut scope = BTreeSet::new();
    for name in selected {
        let name = name.trim();
        if !available.contains(name) {
            return Err(miette!(
                help = did_you_mean(name, &loaded.rhei_ids)
                    .unwrap_or_else(|| "this project holds no rheis.".to_string()),
                "unknown rhei '{}'; this project has: {}",
                name,
                loaded.rhei_ids.join(", ")
            ));
        }
        scope.insert(name.to_string());
    }
    Ok(Some(scope))
}

/// True when a project-qualified ticket id belongs to an in-scope rhei.
/// Narrowing selects candidate tickets only — prior resolution still spans the
/// whole project. §FS-rhei-panta.6.1
fn task_in_rhei_scope(scope: &RheiScope, task_id: &str) -> bool {
    let Some(scope) = scope else { return true };
    let owner = task_id.split_once('.').map(|(head, _)| head).unwrap_or(task_id);
    scope.contains(owner)
}

/// Build a scope set from an already-validated `--rhei` selection. Validation
/// against the project's rhei ids happens once at command entry.
fn rhei_scope_set(selected: &[String]) -> RheiScope {
    if selected.is_empty() {
        None
    } else {
        Some(selected.iter().map(|id| id.trim().to_string()).collect())
    }
}

/// Drop candidate tickets outside the invocation's `--rhei` scope. Applied to
/// the readiness result so priors still resolve across the whole project.
/// §FS-rhei-panta.6.1
fn narrow_to_rhei_scope<'a>(
    tasks: Vec<&'a rhei_core::ast::Task>,
    scope: &RheiScope,
) -> Vec<&'a rhei_core::ast::Task> {
    if scope.is_none() {
        return tasks;
    }
    tasks.into_iter().filter(|task| task_in_rhei_scope(scope, &task.id.to_string())).collect()
}

/// Resolve a CLI ticket target to its project-qualified id: the qualified id
/// itself, or a rhei-local shorthand unambiguous within the scope. A `--rhei`
/// narrowing bounds both. §FS-rhei-panta.6 §FS-rhei-panta.6.1
fn resolve_cli_task_id(
    loaded: &LoadedPlan,
    task_id_str: &str,
    scope: &RheiScope,
) -> MietteResult<String> {
    let target = parse_task_id(task_id_str);
    if find_task_by_id(&loaded.rhei.tasks, &target).is_some() {
        if !task_in_rhei_scope(scope, task_id_str) {
            return Err(miette!(
help = rhei_scope_help(),

                "task '{}' is outside the --rhei scope ({})",
                task_id_str,
                scope_label(scope)
            ));
        }
        return Ok(task_id_str.to_string());
    }
    let candidates: Vec<String> = loaded
        .rhei_ids
        .iter()
        .filter(|rhei_id| scope.as_ref().is_none_or(|scope| scope.contains(*rhei_id)))
        .map(|rhei_id| format!("{rhei_id}.{task_id_str}"))
        .filter(|qualified| {
            find_task_by_id(&loaded.rhei.tasks, &parse_task_id(qualified)).is_some()
        })
        .collect();
    match candidates.len() {
        0 if scope.is_some() => Err(miette!(
help = rhei_scope_help(),

            "task '{}' not found in the --rhei scope ({})",
            task_id_str,
            scope_label(scope)
        )),
        0 => {
            // Ticket ids are project-qualified now; point a user typing a
            // stale or partial id at the closest real ones. §FS-rhei-panta.6
            let scope_noun = if loaded.is_panta_project() { "project" } else { "rhei" };
            let similar = similar_task_ids(loaded, task_id_str);
            if similar.is_empty() {
                // Nothing close enough to suggest — name the next step rather
                // than leaving a dead end.
                Err(miette!(
help = task_id_help(),

                    "task '{}' not found in this {}; `rhei list` shows every ticket id",
                    task_id_str,
                    scope_noun
                ))
            } else {
                Err(miette!(
help = task_id_help(),

                    "task '{}' not found in this {}; closest ids: {}",
                    task_id_str,
                    scope_noun,
                    similar.join(", ")
                ))
            }
        }
        1 => Ok(candidates.into_iter().next().expect("one candidate")),
        _ => Err(miette!(
help = "a qualified id is <rhei>.<ticket>. Copy one from: rhei list",

            "task id '{}' is ambiguous across rheis; use a qualified id: {}",
            task_id_str,
            candidates.join(", ")
        )),
    }
}

/// Qualified ids that contain the attempted id as a substring, for
/// did-you-mean hints. Capped so a large project stays readable.
fn similar_task_ids(loaded: &LoadedPlan, needle: &str) -> Vec<String> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut tasks = Vec::new();
    collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
    tasks
        .iter()
        .map(|task| task.id.to_string())
        .filter(|id| id.contains(needle))
        .take(5)
        .collect()
}

/// Render a scope for diagnostics: the named rheis, or the whole project.
fn scope_label(scope: &RheiScope) -> String {
    match scope {
        Some(scope) => scope.iter().cloned().collect::<Vec<_>>().join(", "),
        None => "whole project".to_string(),
    }
}

/// Resolved write routing for one ticket; see [`LoadedPlan::task_route`].
struct TaskRoute {
    /// File whose headings contain the ticket.
    task_file: PathBuf,
    /// File whose frontmatter owns the ticket's runtime metadata.
    metadata_file: PathBuf,
    /// The ticket id as written inside `task_file`.
    local_id: String,
    /// The `metadata.tasks.<id>` key inside `metadata_file`: rhei-local for an
    /// authored rhei, project-qualified for the synthetic basin, whose
    /// metadata shares the project manifest. §FS-rhei-panta.6.1
    metadata_id: String,
    /// Root directory for the owning rhei's `runtime/` artifacts.
    execution_root: PathBuf,
}

/// Explain that a member rhei validated its whole project.
///
/// Pointing validation at one rhei of a project widens rather than narrows, and
/// silently reporting another rhei's errors under a command the user aimed at
/// this one would be bewildering. Say what happened and why.
// §FS-rhei-validate.1.1: validation takes no `--rhei`, so it widens instead.
fn report_validation_widened(target: &PlanTarget) {
    let Some(id) = target.implied_scope.first() else {
        return;
    };
    println!(
        "Scope: rhei '{id}' belongs to the project at {}, and its state machine, settings, and \
         cross-rhei **Prior:** resolve only there — validating the whole project.",
        display_path(target.path())
    );
}

fn report_panta_scope_narrowed(loaded: &LoadedPlan, command: &str, scope: &RheiScope) {
    if !(loaded.is_panta_project() || loaded.rhei_ids.len() > 1) {
        return;
    }
    let affected: Vec<&str> = loaded
        .rhei_ids
        .iter()
        .map(String::as_str)
        .filter(|id| scope.as_ref().is_none_or(|scope| scope.contains(*id)))
        .collect();
    let qualifier = if scope.is_some() { "narrowed to" } else { "operates project-wide across" };
    let noun = if affected.len() == 1 { "rhei" } else { "rheis" };
    let line = format!(
        "Scope: `rhei {}` {} {} {}: {}",
        command,
        qualifier,
        affected.len(),
        noun,
        affected.join(", ")
    );
    // Prose, so it moves to stderr when stdout is a record stream rather than
    // being dropped: a JSON consumer must not have to parse around it, and an
    // operator watching the run should still see it. §FS-rhei-run-json.1
    if stdout_carries_json_records() {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

/// Load a plan from a file or directory workspace.
fn load_plan(path: &Path) -> MietteResult<LoadedPlan> {
    load_plan_with(path, false, None)
}

/// Load the initial `rhei run` input with enough invocation context to correct
/// a container path that sits above the selected workspace. §FS-rhei-errors.3.2
fn load_plan_for_run(
    path: &Path,
    options: &RunOptions,
    state_machine: Option<&Path>,
) -> MietteResult<LoadedPlan> {
    load_plan_with(path, false, Some(RunPlanInput { options, state_machine }))
}

/// Load a plan, and for a project skip rheis that fail to load rather than
/// failing the whole project. Only read-only surfaces that can report the skip
/// may use this: a partial graph cannot decide readiness. §FS-rhei-panta.6
fn load_plan_leniently(path: &Path) -> MietteResult<LoadedPlan> {
    load_plan_with(path, true, None)
}

fn load_plan_with(
    path: &Path,
    lenient: bool,
    run: Option<RunPlanInput<'_>>,
) -> MietteResult<LoadedPlan> {
    if let Some(project_dir) = workspace::panta_project_dir(path) {
        let project = if lenient {
            workspace::load_panta_project_lenient(&project_dir)
        } else {
            workspace::load_panta_project(&project_dir)
        }
        .map_err(|err| nested_parse_report(&err))?;
        Ok(panta_loaded_plan(project))
    } else if let Some(ws_dir) = workspace::workspace_dir(path) {
        // §AR-rhei-panta.2: a bare Directory Workspace is the single rhei of
        // an implicit Panta; the graph shape matches an explicit project.
        let ws = workspace::load_workspace(&ws_dir).map_err(|err| nested_parse_report(&err))?;
        check_rhei_identity(&ws_dir)?;
        let project = workspace::wrap_rhei_as_implicit_panta(ws, &ws_dir)
            .map_err(|err| nested_parse_report(&err))?;
        Ok(implicit_loaded_plan(project, LoadedPlanKind::Workspace))
    } else if path.is_dir() {
        Err(unrecognized_plan_directory_report(path, run)?)
    } else {
        let input = read_input_file(path)?;
        let rhei = rhei_core::parse(&input).map_err(|err| parse_report(path, &input, &err))?;
        // §AR-rhei-panta.2/.3: a bare rhei file wraps as an implicit Panta
        // with its id derived from the file stem.
        check_rhei_identity(path)?;
        let project = workspace::implicit_panta_from_file_rhei(rhei, path)
            .map_err(|err| nested_parse_report(&err))?;
        Ok(implicit_loaded_plan(project, LoadedPlanKind::SingleFile))
    }
}

/// Build a [`LoadedPlan`] from an implicit-Panta wrap, keeping the original
/// input shape in `kind` for shape-specific behavior (labels, parallel gating).
fn implicit_loaded_plan(
    project: rhei_core::workspace::PantaProject,
    kind: LoadedPlanKind,
) -> LoadedPlan {
    LoadedPlan {
        rhei: project.rhei,
        kind,
        task_sources: project.task_sources,
        task_roots: project.task_roots,
        content_section_roots: project.content_section_roots,
        rhei_ids: project.rhei_ids,
        rhei_machines: project.rhei_machines,
        rhei_roots: project.rhei_roots,
        rhei_titles: project.rhei_titles,
        rhei_plans: project.rhei_plans,
        unloadable: project.unloadable,
    }
}

fn panta_loaded_plan(project: rhei_core::workspace::PantaProject) -> LoadedPlan {
    LoadedPlan {
        rhei: project.rhei,
        kind: LoadedPlanKind::PantaProject,
        task_sources: project.task_sources,
        task_roots: project.task_roots,
        content_section_roots: project.content_section_roots,
        rhei_ids: project.rhei_ids,
        rhei_machines: project.rhei_machines,
        rhei_roots: project.rhei_roots,
        rhei_titles: project.rhei_titles,
        rhei_plans: project.rhei_plans,
        unloadable: project.unloadable,
    }
}

/// Load a prospective project member from hidden staging while qualifying it
/// by its intended published id. §FS-rhei-templates.6.1.2 §AR-rhei-panta.2
fn load_project_with_member_for_validation(
    project: &Path,
    rhei_id: &str,
    staged_entry: &Path,
) -> MietteResult<LoadedPlan> {
    let project = workspace::load_panta_project_with_member(project, rhei_id, staged_entry)
        .map_err(|err| nested_parse_report(&err))?;
    Ok(panta_loaded_plan(project))
}

/// Load a plan for `rhei validate`, collecting recoverable parse errors where
/// validation promises batch diagnostics.
fn load_plan_for_validation(path: &Path) -> MietteResult<LoadedPlan> {
    if let Some(project_dir) = workspace::panta_project_dir(path) {
        let project = workspace::load_panta_project(&project_dir)
            .map_err(|err| nested_parse_report(&err))?;
        return Ok(panta_loaded_plan(project));
    }

    if let Some(ws_dir) = workspace::workspace_dir(path) {
        return load_workspace_for_validation(&ws_dir);
    }

    if path.is_dir() {
        // Validation keeps its collect-errors parser for files, but a real
        // directory with neither manifest is a plan-shape error. §FS-rhei-errors.3.2
        return Err(unrecognized_plan_directory_report(path, None)?);
    }

    let raw = read_input_file(path)?;
    let (maybe_rhei, errs) = rhei_core::parser::parse_collect(&raw);
    match (maybe_rhei, errs.is_empty()) {
        (Some(rhei), true) => {
            check_rhei_identity(path)?;
            let project = workspace::implicit_panta_from_file_rhei(rhei, path)
                .map_err(|err| nested_parse_report(&err))?;
            Ok(implicit_loaded_plan(project, LoadedPlanKind::SingleFile))
        }
        (_, false) | (None, _) => Err(parse_errors_report(path, &raw, &errs)),
    }
}

fn load_workspace_for_validation(ws_dir: &Path) -> MietteResult<LoadedPlan> {
    let index_path = ws_dir.join("index.rhei.md");
    let index_raw = read_input_file(&index_path)?;
    let index = rhei_core::parser::parse_workspace_index(&index_raw)
        .map_err(|err| parse_report(&index_path, &index_raw, &err))?;

    let tasks_dir = ws_dir.join("tasks");
    let mut all_tasks = Vec::new();
    let mut task_sources = HashMap::new();
    let mut parse_error_groups = Vec::new();
    let mut duplicate_task_error: Option<String> = None;

    if tasks_dir.is_dir() {
        let task_files = workspace::discover_task_files(&tasks_dir)
            .map_err(|err| nested_parse_report(&err))?;

        for path in task_files {
            let raw = read_input_file(&path)?;
            let (maybe_tasks, errors) =
                rhei_core::parser::parse_workspace_tasks_collect_with_structure(
                    &raw,
                    &index.structure,
                );
            if !errors.is_empty() {
                parse_error_groups.push(ParseErrorGroup { path, input: raw, errors });
                continue;
            }
            let Some(tasks) = maybe_tasks else {
                continue;
            };
            for task in &tasks {
                if duplicate_task_error.is_none() {
                    if let Err(err) =
                        collect_workspace_task_sources(task, &path, &mut task_sources)
                    {
                        duplicate_task_error = Some(err.message);
                    }
                }
            }
            all_tasks.extend(tasks);
        }
    }

    if !parse_error_groups.is_empty() {
        return Err(workspace_parse_errors_report(&parse_error_groups));
    }
    if let Some(error) = duplicate_task_error {
        return Err(miette!(
            help = "two task files in tasks/ declare the same id. Renumber one of them, then \
                    re-check with: rhei validate <workspace>",
            "{error}"
        ));
    }

    // An empty workspace is a valid, empty rhei; `rhei validate` warns rather
    // than failing the whole project's load. §FS-rhei-plan-language.1.2

    let ws = rhei_core::workspace::Workspace {
        rhei: rhei_core::ast::Rhei {
            title: index.title,
            states: index.states,
            states_declared: index.states_declared,
            structure: index.structure,
            metadata: index.metadata,
            content_sections: index.content_sections,
            tasks: all_tasks,
        },
        task_sources,
    };
    check_rhei_identity(ws_dir)?;
    let project = workspace::wrap_rhei_as_implicit_panta(ws, ws_dir)
        .map_err(|err| nested_parse_report(&err))?;
    Ok(implicit_loaded_plan(project, LoadedPlanKind::Workspace))
}

fn collect_workspace_task_sources(
    task: &rhei_core::ast::Task,
    path: &Path,
    task_sources: &mut HashMap<String, PathBuf>,
) -> rhei_core::parser::Result<()> {
    let id = task.id.to_string();
    if let Some(existing) = task_sources.get(&id) {
        return Err(rhei_core::parser::ParseError::new(
            format!(
                "duplicate task ID '{}': defined in both {} and {}",
                id,
                existing.display(),
                path.display()
            ),
            None,
        ));
    }
    task_sources.insert(id, path.to_path_buf());

    for child in &task.children {
        collect_workspace_task_sources(child, path, task_sources)?;
    }

    Ok(())
}

/// Execute the `validate` subcommand once or in watch mode.
fn validate_command(input: &Path, state_machine: Option<&Path>, watch: bool) -> MietteResult<()> {
    if watch {
        watch_validation_command(input, state_machine)
    } else {
        run_validation_once(input, state_machine)
    }
}

/// Parse a plan, load the selected states, and print validation results.
fn run_validation_once(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    let warnings = validation_warnings_or_error(input, state_machine)?;
    print_validation_report(&warnings);
    Ok(())
}

/// One whole validation pass, before anything decides what to do with it.
///
/// `rhei validate` collapses this straight into a report and a set of warnings,
/// but a command that validates its *own* write compares two passes to tell the
/// errors it introduced from the ones it found — which needs the error strings
/// themselves, not a rendered diagnostic.
// §FS-rhei-new.5.2
struct ValidationPass {
    errors: Vec<String>,
    warnings: Vec<String>,
    /// Guidance the errors carry without owning: the project-global lists a
    /// create elsewhere can change, kept out of the error text so the pre/post
    /// diff stays stable. §FS-rhei-new.5.2
    help: Vec<String>,
    /// Every state-machine source the pass resolved, for an error report to
    /// attribute exactly. §FS-rhei-validate.6
    state_machine_sources: Vec<ValidationMachineSource>,
}

/// Run the whole validation pass, failing on the first error report and
/// returning its warnings otherwise.
///
/// Split out so a command that validates its *own* write can reuse the exact
/// pass `rhei validate` runs without its output — `rhei new` checks the project
/// still loads before it reports success.
// §FS-rhei-new.5.1
fn validation_warnings_or_error(
    input: &Path,
    state_machine: Option<&Path>,
) -> MietteResult<Vec<String>> {
    let pass = validation_pass(input, state_machine)?;
    if !pass.errors.is_empty() {
        return Err(validation_report(
            input,
            &pass.state_machine_sources,
            &pass.errors,
            &pass.help,
        ));
    }
    Ok(pass.warnings)
}

/// The validation pass itself, reported as data. §FS-rhei-new.5.2
fn validation_pass(input: &Path, state_machine: Option<&Path>) -> MietteResult<ValidationPass> {
    let loaded = load_plan_for_validation(input)?;

    validation_pass_for_loaded(input, state_machine, loaded)
}

/// Validate a preloaded graph through the same machine, link, settings, and
/// snapshot checks as `rhei validate`. §FS-rhei-templates.6.1.2
fn validation_pass_for_loaded(
    input: &Path,
    state_machine: Option<&Path>,
    loaded: LoadedPlan,
) -> MietteResult<ValidationPass> {

    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine)?;
    let machines = resolved.validator_set();
    let normalized_input = normalize_workspace_input(input);
    let base_path = if normalized_input.is_dir() {
        normalized_input.as_path()
    } else {
        normalized_input.parent().unwrap_or(Path::new("."))
    };
    let mut report = if loaded.is_panta_project() {
        // Panta task links validate against each ticket's owning rhei root. §AR-rhei-panta.5
        rhei_validator::validate_with_machine_set_and_link_bases(
            &loaded.rhei,
            &machines,
            base_path,
            &loaded.task_roots,
            &loaded.content_section_roots,
        )
    } else {
        let mut report = rhei_validator::Validator::with_machines(machines.clone())
            .validate_with_base(&loaded.rhei, Some(base_path));
        report.warnings.dedup();
        report
    };
    let workspace_root = execution_workspace_root(input);
    let settings = load_merged_settings(&workspace_root)?;
    report
        .errors
        .extend(validate_plan_settings_references(&loaded.rhei, &machines, &settings));
    report.errors.extend(validate_snapshot_plan_context(&loaded, &resolved));
    report.warnings.extend(snapshot_orphan_validation_warnings(
        &workspace_root,
        &loaded,
        &resolved,
        &settings,
    )?);
    // §FS-rhei-panta.6: an empty project is valid, but say discovery found
    // nothing — a misnamed or misplaced plan is otherwise silently invisible
    // behind a green validation.
    if loaded.is_panta_project() && loaded.rhei_ids.is_empty() {
        report.warnings.push(format!(
            "the project holds no rheis: discovery looks only for `*.rhei.md` files and \
             workspace directories placed directly next to index.panta.md — {}",
            add_a_rhei_hint()
        ));
    }
    // An empty rhei is valid, but a mistyped `tasks/` looks identical to a
    // deliberately empty one — name it rather than let it pass unremarked.
    // §FS-rhei-plan-language.1.2
    report.warnings.extend(empty_rhei_warnings(&loaded));
    report.warnings.extend(ignored_member_settings_warnings(input, &loaded));

    report.help.dedup();
    Ok(ValidationPass {
        errors: report.errors,
        warnings: report.warnings,
        help: report.help,
        state_machine_sources: resolved.validation_sources(&loaded.rhei_ids),
    })
}

/// Print success output and any non-fatal validation warnings.
fn print_validation_report(warnings: &[String]) {
    println!("Validation succeeded");
    for warning in warnings {
        println!("warning: {warning}");
    }
}

/// Watch the plan and states files and re-run validation on relevant changes.
fn watch_validation_command(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    let mut watch_plan = validation_watch_plan(input, state_machine);

    println!(
        "Watch mode started for '{}' (states: {})",
        input.display(),
        watch_plan.state_machine_label,
    );

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .map_err(|err| miette!(
        help = watch_help(),
        "failed to initialize file watcher: {err}"
    ))?;

    let mut watched = Vec::new();
    register_watch_roots(&mut watcher, &watch_plan.roots, &mut watched)?;

    run_validation_pass(input, state_machine);

    loop {
        let event = match rx.recv() {
            Ok(Ok(event)) => event,
            Ok(Err(err)) => {
                eprintln!("watch error: {err}");
                continue;
            }
            Err(err) => return Err(miette!(
                help = watch_help(),
                "watch channel disconnected: {err}"
            )),
        };

        if !should_revalidate(&event, &watch_plan.targets) {
            continue;
        }

        while debounce_has_relevant_event(&rx, &watch_plan.targets) {}

        println!("--- change detected, revalidating ---");
        run_validation_pass(input, state_machine);

        // `prompt_templates/` is optional, so the initial plan may have had no
        // directory to watch recursively. Re-plan after every pass: once the
        // author creates it, the next pass picks up edits to the files inside
        // instead of watching a directory that no longer describes the tree.
        watch_plan = validation_watch_plan(input, state_machine);
        register_watch_roots(&mut watcher, &watch_plan.roots, &mut watched)?;
    }
}

/// Watch every planned root that is not already being watched.
fn register_watch_roots(
    watcher: &mut RecommendedWatcher,
    roots: &[WatchRoot],
    watched: &mut Vec<WatchRoot>,
) -> MietteResult<()> {
    for root in roots {
        if watched.contains(root) {
            continue;
        }
        watcher.watch(&root.path, root.mode).map_err(|err| {
            miette!(help = watch_help(), "failed to watch '{}': {err}", root.path.display())
        })?;
        watched.push(root.clone());
    }
    Ok(())
}

/// Run one validation pass in watch mode, writing any failure to stderr.
fn run_validation_pass(input: &Path, state_machine: Option<&Path>) {
    if let Err(err) = run_validation_once(input, state_machine) {
        eprintln!("{err:?}");
    }
}

fn debounce_has_relevant_event(
    rx: &mpsc::Receiver<notify::Result<Event>>,
    targets: &[WatchTarget],
) -> bool {
    match rx.recv_timeout(Duration::from_millis(250)) {
        Ok(Ok(event)) => should_revalidate(&event, targets),
        Ok(Err(err)) => {
            eprintln!("watch error: {err}");
            false
        }
        Err(RecvTimeoutError::Timeout) => false,
        Err(RecvTimeoutError::Disconnected) => false,
    }
}

fn should_revalidate(event: &Event, targets: &[WatchTarget]) -> bool {
    if !is_relevant_event_kind(&event.kind) {
        return false;
    }

    event.paths.iter().any(|path| path_matches(path, targets))
}

fn is_relevant_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn path_matches(path: &Path, targets: &[WatchTarget]) -> bool {
    // Exclusions win: a path inside an excluded artifact tree never revalidates,
    // even though it sits under a watched descendant root.
    if targets
        .iter()
        .any(|target| matches!(target, WatchTarget::ExcludedDir(name) if path_has_component(path, name)))
    {
        return false;
    }
    targets.iter().any(|target| match target {
        WatchTarget::Exact(watched) => paths_equivalent(path, watched),
        WatchTarget::Descendant(root) | WatchTarget::OptionalDescendant(root) => {
            path_is_under(path, root)
        }
        WatchTarget::ExcludedDir(_) => false,
    })
}

/// True if any path component is exactly `name` (e.g. a `runtime` directory
/// anywhere in the path).
fn path_has_component(path: &Path, name: &str) -> bool {
    path.components().any(|component| component.as_os_str() == name)
}

fn paths_equivalent(candidate: &Path, watched: &Path) -> bool {
    match (normalize_path(candidate), normalize_path(watched)) {
        (Some(candidate), Some(watched)) => return candidate == watched,
        (Some(candidate), None) if candidate == watched => return true,
        (None, Some(watched)) if candidate == watched => return true,
        (None, None) => {}
        _ => {}
    }

    let candidate_file_name = candidate.file_name();
    let watched_file_name = watched.file_name();

    candidate_file_name.is_some()
        && candidate_file_name == watched_file_name
        && candidate.components().last() == watched.components().last()
}

fn path_is_under(candidate: &Path, root: &Path) -> bool {
    match (normalize_path(candidate), normalize_path(root)) {
        (Some(candidate), Some(root)) => candidate.starts_with(root),
        _ => candidate.starts_with(root),
    }
}

#[derive(Debug, Clone)]
struct ValidationWatchPlan {
    targets: Vec<WatchTarget>,
    roots: Vec<WatchRoot>,
    state_machine_label: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum WatchTarget {
    Exact(PathBuf),
    Descendant(PathBuf),
    OptionalDescendant(PathBuf),
    /// Ignore any event whose path passes through a directory with this name,
    /// at any depth (e.g. a `runtime/` artifact tree the tools write into —
    /// including the per-rhei `runtime/` trees nested under workspace rheis).
    ExcludedDir(&'static str),
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct WatchRoot {
    path: PathBuf,
    mode: RecursiveMode,
}

fn validation_watch_plan(input: &Path, state_machine: Option<&Path>) -> ValidationWatchPlan {
    let state_machine_path = state_machine.map(Path::to_path_buf).or_else(|| {
        let candidate = watch_auto_state_machine_path(input);
        if candidate.is_file() { Some(candidate) } else { None }
    });
    let state_machine_label = state_machine_label(state_machine_path.as_deref());

    let mut targets = plan_watch_targets(input);
    let watched_state_machine_path = if let Some(path) = state_machine {
        path.to_path_buf()
    } else {
        watch_auto_state_machine_path(input)
    };
    targets.push(WatchTarget::Exact(canonical_watch_path(&watched_state_machine_path)));
    targets.push(WatchTarget::OptionalDescendant(canonical_watch_path(
        &rhei_validator::prompt_templates_dir(&watched_state_machine_path),
    )));

    let mut roots = Vec::new();
    for target in &targets {
        add_watch_root_for_target(&mut roots, target);
    }

    ValidationWatchPlan { targets, roots, state_machine_label }
}

fn plan_watch_targets(input: &Path) -> Vec<WatchTarget> {
    if let Some(project_root) = workspace::panta_project_dir(input) {
        return panta_watch_targets(&project_root);
    }

    if let Some(workspace_root) = workspace::workspace_dir(input) {
        return workspace_watch_targets(&workspace_root);
    }

    if input.is_dir() {
        workspace_watch_targets(input)
    } else {
        vec![WatchTarget::Exact(canonical_watch_path(input))]
    }
}

fn panta_watch_targets(project_root: &Path) -> Vec<WatchTarget> {
    vec![
        // The project directory holds the manifest, every rhei, and the synthetic
        // basin, so one descendant watch covers them all. §AR-rhei-panta.1
        WatchTarget::Descendant(canonical_watch_path(project_root)),
        // Exclude every `runtime/` artifact tree (the project's and the per-rhei
        // ones nested under workspace rheis) at any depth, so a re-render that
        // writes there never re-triggers itself. §AR-rhei-panta.5
        WatchTarget::ExcludedDir("runtime"),
    ]
}

fn workspace_watch_targets(workspace_root: &Path) -> Vec<WatchTarget> {
    vec![
        WatchTarget::Exact(canonical_watch_path(&workspace_root.join("index.rhei.md"))),
        WatchTarget::Descendant(canonical_watch_path(&workspace_root.join("tasks"))),
    ]
}

fn watch_auto_state_machine_path(input: &Path) -> PathBuf {
    if let Some(project_root) = workspace::panta_project_dir(input) {
        project_root.join("states.yaml")
    } else if let Some(workspace_root) = workspace::workspace_dir(input) {
        workspace_root.join("states.yaml")
    } else if input.is_dir() {
        input.join("states.yaml")
    } else {
        input.parent().unwrap_or_else(|| Path::new(".")).join("states.yaml")
    }
}

#[cfg(test)]
fn canonical_watched_paths(input: &Path, state_machine: &Path) -> Vec<WatchTarget> {
    let mut targets = plan_watch_targets(input);
    targets.push(WatchTarget::Exact(canonical_watch_path(state_machine)));
    targets.push(WatchTarget::OptionalDescendant(canonical_watch_path(
        &rhei_validator::prompt_templates_dir(state_machine),
    )));
    targets
}

fn add_watch_root_for_target(roots: &mut Vec<WatchRoot>, target: &WatchTarget) {
    let (path, mode) = match target {
        // An exclusion only filters events; it is not itself a watch root.
        WatchTarget::ExcludedDir(_) => return,
        WatchTarget::Exact(path) => {
            let root = path.parent().unwrap_or_else(|| Path::new("."));
            (canonical_watch_path(root), RecursiveMode::NonRecursive)
        }
        WatchTarget::Descendant(path) => {
            if path.is_dir() {
                (canonical_watch_path(path), RecursiveMode::Recursive)
            } else {
                let root = path.parent().unwrap_or_else(|| Path::new("."));
                (canonical_watch_path(root), RecursiveMode::Recursive)
            }
        }
        WatchTarget::OptionalDescendant(path) => {
            if path.is_dir() {
                (canonical_watch_path(path), RecursiveMode::Recursive)
            } else {
                let root = path.parent().unwrap_or_else(|| Path::new("."));
                (canonical_watch_path(root), RecursiveMode::NonRecursive)
            }
        }
    };

    // A recursive watch already covers everything beneath it, so a non-recursive
    // root on the same path or a descendant is redundant (e.g. the Panta project
    // dir watched recursively also covers its `index.panta.md` and `states.yaml`).
    if mode == RecursiveMode::NonRecursive
        && roots
            .iter()
            .any(|existing| existing.mode == RecursiveMode::Recursive && path.starts_with(&existing.path))
    {
        return;
    }
    // Conversely, a new recursive root supersedes any non-recursive roots beneath it.
    if mode == RecursiveMode::Recursive {
        roots.retain(|existing| {
            !(existing.mode == RecursiveMode::NonRecursive && existing.path.starts_with(&path))
        });
    }

    let root = WatchRoot { path, mode };
    if !roots.iter().any(|existing| existing == &root) {
        roots.push(root);
    }
}

fn canonical_watch_path(path: &Path) -> PathBuf {
    if let Some(normalized) = normalize_path(path) {
        return normalized;
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
    };

    let Some(parent) = absolute.parent() else {
        return absolute;
    };
    let Some(normalized_parent) = normalize_path(parent) else {
        return absolute;
    };

    absolute
        .file_name()
        .map(|name| normalized_parent.join(name))
        .unwrap_or(normalized_parent)
}

fn normalize_path(path: &Path) -> Option<PathBuf> {
    rhei_core::platform::canonical_path(path).ok()
}
