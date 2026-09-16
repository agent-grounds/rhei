fn completions_command(
    shell: Option<CompletionShell>,
    install: bool,
    system: bool,
    output: Option<&Path>,
    dry_run: bool,
) -> MietteResult<()> {
    let shell = resolve_completion_shell(shell)?;
    if install || output.is_some() || dry_run {
        let path = match output {
            Some(path) => path.to_path_buf(),
            None => completion_install_path(shell, system, &invoked_bin_name())?,
        };
        if dry_run {
            println!("Would install {} completions to {}", shell.as_str(), path.display());
            return Ok(());
        }

        write_completion_file(shell, &path)?;
        println!("Installed {} completions to {}", shell.as_str(), path.display());
        return Ok(());
    }

    let mut stdout = std::io::stdout();
    write_completion_script(shell, &mut stdout)?;
    Ok(())
}

/// How to enable the generated script, as comments in the script itself. The
/// instructions name the binary the user actually ran, so the `rh` alias does
/// not tell them to rerun `rhei`. §FS-rhei-completions.5 §FS-rhei-distribution.1
fn completion_header(shell: CompletionShell, bin: &str) -> String {
    let shell_name = shell.as_str();
    let (source_line, rc_file) = match shell {
        CompletionShell::Bash => {
            (format!("source <({bin} completions bash)"), "~/.bashrc")
        }
        CompletionShell::Zsh => (format!("source <({bin} completions zsh)"), "~/.zshrc"),
        CompletionShell::Fish => {
            (format!("{bin} completions fish | source"), "~/.config/fish/config.fish")
        }
        CompletionShell::PowerShell => (
            format!("{bin} completions powershell | Out-String | Invoke-Expression"),
            "$PROFILE",
        ),
        CompletionShell::Elvish => {
            (format!("eval ({bin} completions elvish | slurp)"), "~/.config/elvish/rc.elv")
        }
    };
    let mut header = format!(
        "# {bin} completions for {shell}.\n\
         # Enable in the current shell:\n\
         #   {source_line}\n\
         # Enable permanently: add that line to {rc_file}, or install a completion file:\n\
         #   {bin} completions {shell} --install\n",
        shell = shell_name,
    );
    if matches!(shell, CompletionShell::Zsh) {
        header.push_str(
            "#   (installs to ~/.zfunc; make sure ~/.zfunc is on fpath before compinit)\n",
        );
    }
    header
}

/// Writes the comment header plus the registration script, keeping a `#compdef`
/// directive on the first line so the Zsh output stays autoloadable. §FS-rhei-completions.5
fn write_completion_script(
    shell: CompletionShell,
    writer: &mut dyn std::io::Write,
) -> MietteResult<()> {
    let mut registration = Vec::new();
    write_completion_registration(shell, &mut registration)?;
    let registration = String::from_utf8_lossy(&registration);
    let header = completion_header(shell, &invoked_bin_name());

    let script = match registration.split_once('\n') {
        Some((first, rest)) if first.starts_with("#compdef") => {
            format!("{first}\n{header}{rest}")
        }
        _ => format!("{header}{registration}"),
    };
    writer
        .write_all(script.as_bytes())
        .map_err(|err| miette!(
help = "completions are written to stdout; redirect them to a file your shell sources.",
"failed to write {} completions: {err}", shell.as_str()))
}

/// Falls back to the shell detected from `$SHELL` when none was given. §FS-rhei-completions.2
fn resolve_completion_shell(shell: Option<CompletionShell>) -> MietteResult<CompletionShell> {
    if let Some(shell) = shell {
        return Ok(shell);
    }
    match detect_current_shell(std::env::var_os("SHELL").as_deref()) {
        Some(shell) => {
            eprintln!("No shell given; using {} (detected from $SHELL)", shell.as_str());
            Ok(shell)
        }
        None => Err(miette!(
            help = "supported shells: bash, zsh, fish, powershell, elvish\n\
                    example: rhei completions zsh --install",
            "could not detect a supported shell from $SHELL; pass one explicitly"
        )),
    }
}

/// Maps the basename of `$SHELL` to a supported shell. §FS-rhei-completions.2
fn detect_current_shell(shell_var: Option<&OsStr>) -> Option<CompletionShell> {
    match Path::new(shell_var?).file_name()?.to_str()? {
        "bash" => Some(CompletionShell::Bash),
        "zsh" => Some(CompletionShell::Zsh),
        "fish" => Some(CompletionShell::Fish),
        "pwsh" | "powershell" => Some(CompletionShell::PowerShell),
        "elvish" => Some(CompletionShell::Elvish),
        _ => None,
    }
}

fn write_completion_file(shell: CompletionShell, path: &Path) -> MietteResult<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create completions directory", err))?;
    }

    let mut buffer = Vec::new();
    write_completion_script(shell, &mut buffer)?;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|err| {
        file_io_report(parent, "failed to create temporary completions file", err)
    })?;
    temp.write_all(&buffer)
        .map_err(|err| file_io_report(path, "failed to write completions file", err))?;
    temp.flush().map_err(|err| file_io_report(path, "failed to flush completions file", err))?;
    temp.persist(path)
        .map_err(|err| file_io_report(path, "failed to install completions file", err.error))?;
    Ok(())
}

/// The name this process was invoked under, so the `rh` alias registers and
/// completes under its own name instead of `rhei`. §FS-rhei-distribution.1
fn invoked_bin_name() -> String {
    std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(Path::file_stem)
        .and_then(OsStr::to_str)
        .map(str::to_owned)
        .unwrap_or_else(|| "rhei".to_string())
}

fn write_completion_registration(
    shell: CompletionShell,
    writer: &mut dyn std::io::Write,
) -> MietteResult<()> {
    let bin = invoked_bin_name();
    let completer = std::env::current_exe()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .unwrap_or_else(|| bin.clone());
    completion_env_completer(shell)
        .write_registration("COMPLETE", &bin, &bin, &completer, writer)
        .map_err(|err| miette!(
            help = "see how to enable completions with: rhei completions --help",
            "failed to generate {} completions: {err}", shell.as_str()
        ))
}

fn completion_env_completer(shell: CompletionShell) -> &'static dyn EnvCompleter {
    match shell {
        CompletionShell::Bash => &CompletionBash,
        CompletionShell::Zsh => &CompletionZsh,
        CompletionShell::Fish => &CompletionFish,
        CompletionShell::PowerShell => &CompletionPowerShell,
        CompletionShell::Elvish => &CompletionElvish,
    }
}

/// Where an installed completion file belongs. The filename carries the
/// invoked binary name so installing for the `rh` alias adds a second file
/// instead of overwriting `rhei`'s. §FS-rhei-distribution.1
fn completion_install_path(
    shell: CompletionShell,
    system: bool,
    bin: &str,
) -> MietteResult<PathBuf> {
    if system {
        return Ok(match shell {
            CompletionShell::Bash => {
                PathBuf::from("/usr/local/share/bash-completion/completions").join(bin)
            }
            CompletionShell::Zsh => {
                PathBuf::from("/usr/local/share/zsh/site-functions").join(format!("_{bin}"))
            }
            CompletionShell::Fish => PathBuf::from("/usr/local/share/fish/vendor_completions.d")
                .join(format!("{bin}.fish")),
            CompletionShell::PowerShell => PathBuf::from("/usr/local/share/powershell/Completions")
                .join(format!("{bin}-completions.ps1")),
            CompletionShell::Elvish => {
                PathBuf::from("/usr/local/share/elvish/lib").join(format!("{bin}-completions.elv"))
            }
        });
    }

    Ok(match shell {
        CompletionShell::Bash => xdg_data_home()?.join("bash-completion/completions").join(bin),
        CompletionShell::Zsh => home_dir()?.join(".zfunc").join(format!("_{bin}")),
        CompletionShell::Fish => {
            xdg_config_home()?.join("fish/completions").join(format!("{bin}.fish"))
        }
        CompletionShell::PowerShell => {
            xdg_config_home()?.join("powershell").join(format!("{bin}-completions.ps1"))
        }
        CompletionShell::Elvish => {
            xdg_config_home()?.join("elvish/lib").join(format!("{bin}-completions.elv"))
        }
    })
}

fn complete_any_path(current: &OsStr) -> Vec<CompletionCandidate> {
    PathCompleter::any().complete(current)
}

fn complete_yaml_path(current: &OsStr) -> Vec<CompletionCandidate> {
    complete_path_with_extensions(current, &["yaml", "yml"])
}

fn complete_values_path(current: &OsStr) -> Vec<CompletionCandidate> {
    complete_path_with_extensions(current, &["yaml", "yml", "json"])
}

fn complete_rhei_plan_path(current: &OsStr) -> Vec<CompletionCandidate> {
    let current_path = Path::new(current);
    let parent = current_path.parent().filter(|p| !p.as_os_str().is_empty());
    let file_prefix =
        current_path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let dir = parent.unwrap_or_else(|| Path::new("."));
    let mut candidates = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&file_prefix) {
                continue;
            }
            let include = if path.is_dir() { true } else { name.ends_with(".rhei.md") };
            if include {
                candidates.push(path_completion_candidate(parent, &name, path.is_dir()));
            }
        }
    }

    candidates
}

fn complete_path_with_extensions(current: &OsStr, extensions: &[&str]) -> Vec<CompletionCandidate> {
    let current_path = Path::new(current);
    let parent = current_path.parent().filter(|p| !p.as_os_str().is_empty());
    let file_prefix =
        current_path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let dir = parent.unwrap_or_else(|| Path::new("."));
    let mut candidates = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&file_prefix) {
                continue;
            }
            let include = if path.is_dir() {
                true
            } else {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| extensions.iter().any(|allowed| ext == *allowed))
            };
            if include {
                candidates.push(path_completion_candidate(parent, &name, path.is_dir()));
            }
        }
    }

    candidates
}

fn path_completion_candidate(
    parent: Option<&Path>,
    name: &str,
    is_dir: bool,
) -> CompletionCandidate {
    let mut value = parent.map(|p| p.join(name)).unwrap_or_else(|| PathBuf::from(name));
    if is_dir {
        value.push("");
    }
    CompletionCandidate::new(value.into_os_string())
}

fn complete_template_source(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("all", "Project, user, and built-in templates"),
            ("project", "Project templates only"),
            ("user", "User templates only"),
            ("builtin", "Templates shipped with the rhei binary"),
        ],
    )
}

fn complete_parallel(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("1", "One task at a time"),
            ("2", "Two concurrent tasks"),
            ("4", "Four concurrent tasks"),
            ("8", "Eight concurrent tasks"),
            ("0", "Unlimited concurrency"),
        ],
    )
}

fn complete_duration(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("30s", "Thirty seconds"),
            ("1m", "One minute"),
            ("5m", "Five minutes"),
            ("15m", "Fifteen minutes"),
            ("1h", "One hour"),
        ],
    )
}

fn complete_limit(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("10", "Ten tasks"),
            ("25", "Twenty-five tasks"),
            ("50", "Fifty tasks"),
            ("100", "One hundred tasks"),
            ("0", "No limit"),
        ],
    )
}

fn complete_skill_name(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("rhei-plan-writer", "Create and refactor Rhei Plan documents"),
            ("rhei-plan-worker", "Execute tasks in Rhei Plan documents"),
            ("rhei-state-machine-writer", "Design custom Rhei state machines"),
            ("rhei-template-writer", "Create and edit reusable Rhei Templates"),
        ],
    )
}

fn static_completion(current: &OsStr, values: &[(&str, &str)]) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    values
        .iter()
        .filter(|(value, _)| value.starts_with(prefix.as_ref()))
        .map(|(value, help)| {
            CompletionCandidate::new((*value).to_string()).help(Some((*help).to_string().into()))
        })
        .collect()
}

fn complete_agent_name(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let settings = load_merged_settings_for_completion(&completion_workspace_root());
    settings
        .agents
        .keys()
        .filter(|name| name.starts_with(prefix.as_ref()))
        .map(|name| CompletionCandidate::new(name.clone()).help(Some("Configured agent".into())))
        .collect()
}

fn complete_agent_mode(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let settings = load_merged_settings_for_completion(&completion_workspace_root());
    let selected_agent = completion_option_value("agent");
    let mut modes = BTreeSet::new();
    if let Some(agent) = selected_agent.as_deref().and_then(|agent| settings.agents.get(agent)) {
        modes.extend(agent.modes.keys().cloned());
    } else {
        for agent in settings.agents.values() {
            modes.extend(agent.modes.keys().cloned());
        }
    }
    modes
        .into_iter()
        .filter(|mode| mode.starts_with(prefix.as_ref()))
        .map(|mode| CompletionCandidate::new(mode).help(Some("Agent mode".into())))
        .collect()
}

fn complete_model_name(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let mut models = BTreeSet::new();
    if let Some(model) = load_merged_settings_for_completion(&completion_workspace_root()).model {
        models.insert(model);
    }
    if let Some(machines) = completion_state_machines() {
        for machine in machines.distinct() {
            models.extend(machine.models.iter().cloned());
            for state in machine.states.values() {
                if let Some(model) = state.model.as_ref() {
                    models.insert(model.clone());
                }
                models.extend(state.all_models.iter().cloned());
            }
        }
    }
    models
        .into_iter()
        .filter(|model| model.starts_with(prefix.as_ref()))
        .map(|model| CompletionCandidate::new(model).help(Some("Configured model".into())))
        .collect()
}

fn complete_assignee(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    let mut counts = BTreeMap::<String, usize>::new();
    for task in flatten_tasks(&loaded.rhei) {
        if let Some(assignee) = &task.assignee {
            *counts.entry(assignee.clone()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(assignee, _)| assignee.starts_with(prefix.as_ref()))
        .map(|(assignee, count)| {
            CompletionCandidate::new(assignee).help(Some(task_count_help(count).into()))
        })
        .collect()
}

fn complete_node_kind(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy().to_ascii_lowercase();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    let mut counts = BTreeMap::<String, usize>::new();
    for task in flatten_tasks(&loaded.rhei) {
        *counts.entry(task.kind.clone()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(kind, _)| kind.starts_with(&prefix))
        .map(|(kind, count)| {
            CompletionCandidate::new(kind).help(Some(task_count_help(count).into()))
        })
        .collect()
}

fn task_count_help(count: usize) -> String {
    match count {
        1 => "1 matching task".to_string(),
        n => format!("{n} matching tasks"),
    }
}

fn complete_task_id(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    flatten_tasks(&loaded.rhei)
        .into_iter()
        .filter_map(|task| {
            let id = task.id.to_string();
            id.starts_with(prefix.as_ref()).then(|| {
                CompletionCandidate::new(id)
                    .help(Some(format!("{} [{}]", task.title, task.state).into()))
            })
        })
        .collect()
}

/// How many ended runs completion offers. The registry keeps a hundred; a
/// hundred candidates is a list nobody reads.
// §FS-rhei-run-headless.2
const COMPLETED_RUNS_OFFERED: usize = 10;

/// The run ids `rhei attach <TAB>` and `rhei stop <TAB>` offer.
///
/// Everything `resolve_run` can reach, in the order it tries: the runs that are
/// live, the ones whose liveness could not be decided — which resolve and are
/// exactly what an operator needs to reach when something is wrong — and then
/// the most recent ended runs, which `rhei attach` answers for too.
///
/// Read without pruning: a tab keypress is not a request to unlink anything.
// §FS-rhei-run-headless.3 §FS-rhei-run-headless.2
fn complete_run_reference(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let registry = read_run_registry();
    let live = registry.live.iter().map(|run| (run, "live"));
    let undecided = registry
        .undecided
        .iter()
        .filter_map(|entry| entry.descriptor.as_ref())
        .map(|run| (run, "unchecked"));
    let ended = registry.ended.iter().take(COMPLETED_RUNS_OFFERED).map(|run| (run, "ended"));
    live.chain(undecided)
        .chain(ended)
        .filter(|(run, _)| run.id.starts_with(prefix.as_ref()))
        .map(|(run, state)| {
            let help = format!("{} [{state}]", run.plan.display());
            CompletionCandidate::new(run.id.clone()).help(Some(help.into()))
        })
        .collect()
}

/// Complete `--rhei` values with the loaded project's rhei ids.
// §FS-rhei-panta.6
fn complete_rhei_id(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    loaded
        .rhei_ids
        .iter()
        .filter(|id| id.starts_with(prefix.as_ref()))
        .map(|id| CompletionCandidate::new(id.clone()))
        .collect()
}

fn complete_transition_from_state(current: &OsStr) -> Vec<CompletionCandidate> {
    if let (Some(plan), Some(task_id)) = (completion_plan_path(), completion_option_value("task")) {
        if let Ok(state) = current_task_state(&plan, &task_id) {
            if state.starts_with(current.to_string_lossy().as_ref()) {
                return vec![
                    CompletionCandidate::new(state).help(Some("Current task state".into()))
                ];
            }
        }
    }
    complete_state_name(current)
}

fn complete_transition_to_state(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(machines) = completion_state_machines() else {
        return Vec::new();
    };
    // A transition targets one ticket; its owning machine names the states.
    // §DA-per-rhei-state-machines
    let machine = completion_option_value("task")
        .map(|task| machines.for_task_str(&task).clone())
        .unwrap_or_else(|| machines.default.clone());
    let from = completion_option_value("from").or_else(|| {
        completion_plan_path()
            .zip(completion_option_value("task"))
            .and_then(|(plan, task)| current_task_state(&plan, &task).ok())
    });
    let mut targets = BTreeSet::new();
    if let Some(from) = from {
        let normalized = normalized_state_name(&from, &machine);
        for rule in machine.transitions() {
            if machine.transition_matches_source(rule, &normalized) {
                targets.insert(rule.to.0.clone());
            }
        }
    } else {
        targets.extend(machine.states.keys().cloned());
    }
    let prefix = current.to_string_lossy();
    targets
        .into_iter()
        .filter(|state| state.starts_with(prefix.as_ref()))
        .map(|state| {
            let help = machine.states.get(&state).and_then(|def| def.description.clone());
            CompletionCandidate::new(state).help(help.map(Into::into))
        })
        .collect()
}

fn complete_comma_state_name(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let (base, prefix) = match current.rsplit_once(',') {
        Some((base, prefix)) => (format!("{base},"), prefix),
        None => (String::new(), current.as_ref()),
    };
    complete_state_name_with_prefix(prefix)
        .into_iter()
        .map(|(state, help)| {
            CompletionCandidate::new(format!("{base}{state}")).help(help.map(Into::into))
        })
        .collect()
}

fn complete_state_name(current: &OsStr) -> Vec<CompletionCandidate> {
    complete_state_name_with_prefix(current.to_string_lossy().as_ref())
        .into_iter()
        .map(|(state, help)| CompletionCandidate::new(state).help(help.map(Into::into)))
        .collect()
}

fn complete_state_name_with_prefix(prefix: &str) -> Vec<(String, Option<String>)> {
    let Some(machines) = completion_state_machines() else {
        return Vec::new();
    };
    // Completion offers the union across every machine in scope; the target
    // command validates against the right ticket's machine.
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for machine in machines.distinct() {
        for (state, def) in &machine.states {
            if state.starts_with(prefix) && seen.insert(state.clone()) {
                out.push((state.clone(), def.description.clone()));
            }
        }
    }
    out
}

fn completion_state_machines() -> Option<rhei_validator::MachineSet> {
    let state_machine = completion_option_value("state-machine").map(PathBuf::from);
    let plan = completion_plan_path();
    match (plan.as_deref(), state_machine.as_deref()) {
        (Some(plan), sm) => load_plan(plan)
            .ok()
            .and_then(|loaded| resolve_state_machines_for_loaded_plan(plan, &loaded, sm).ok())
            .map(|resolved| resolved.validator_set()),
        (None, Some(sm)) => {
            load_state_machine(Some(sm)).ok().map(rhei_validator::MachineSet::single)
        }
        (None, None) => {
            Some(rhei_validator::MachineSet::single(rhei_validator::StateMachine::builtin_default()))
        }
    }
}

fn completion_workspace_root() -> PathBuf {
    completion_plan_path()
        .map(|path| execution_workspace_root(&path))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}
