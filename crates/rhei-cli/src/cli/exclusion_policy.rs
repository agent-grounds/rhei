// Resolution and application of one task's negative read contract.
//
// Authored entries stay in rhei-core; this module is the single filesystem-
// aware policy used by validation, composition, and process adapters.

// §AR-source-file-size.3 §FS-rhei-plan-language.3.13 §FS-rhei-memory.4.1

#[derive(Clone, Debug)]
struct ResolvedExclusion {
    authored: String,
    logical: PathBuf,
    canonical: PathBuf,
    recursive: bool,
}

/// Resolved immediately before an invocation, so symlink changes cannot make
/// validation-time identity stale. §FS-rhei-agents.5.2.1
#[derive(Clone, Debug, Default)]
struct ResolvedExclusions {
    entries: Vec<ResolvedExclusion>,
}

impl ResolvedExclusions {
    /// Whether Rhei may read bytes from `path` while composing this prompt.
    /// Identity and navigation callers do not use this predicate.
    // §FS-rhei-memory.4.1
    fn allows(&self, path: &Path) -> bool {
        let logical = absolute_normalized(path);
        let canonical = canonicalize_longest_existing(&logical).unwrap_or_else(|_| logical.clone());
        !self.entries.iter().any(|entry| {
            exclusion_path_matches(&entry.logical, entry.recursive, &logical)
                || exclusion_path_matches(&entry.canonical, entry.recursive, &canonical)
        })
    }

    /// Every logical and canonical target the enforcing adapter must deny.
    /// Stable de-duplication keeps flag order deterministic.
    // §FS-rhei-agents.1.1.2 §FS-rhei-agents.4.1
    fn adapter_paths(&self) -> Vec<PathBuf> {
        let mut seen = BTreeSet::new();
        let mut paths = Vec::new();
        for entry in &self.entries {
            for path in [&entry.logical, &entry.canonical] {
                if seen.insert(path.clone()) {
                    paths.push(path.clone());
                }
            }
        }
        paths
    }

    fn render(&self, filesystem_denied: bool) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let guarantee = if filesystem_denied {
            "filesystem denied by the selected agent adapter"
        } else {
            "composition only; paths remain readable outside Rhei-composed context"
        };
        let mut out = String::from("\n## Exclusions\n");
        for entry in &self.entries {
            out.push_str(&format!("\n- `{}` — `{}`\n", entry.authored, entry.logical.display()));
        }
        out.push_str(&format!("\nEnforcement: {guarantee}.\n"));
        out
    }

    fn dry_run_suffix(&self, filesystem_denied: bool) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let guarantee = if filesystem_denied { "filesystem denied" } else { "composition only" };
        format!(
            " [excludes: {}; enforcement: {guarantee}]",
            self.entries.iter().map(|entry| entry.authored.as_str()).collect::<Vec<_>>().join(", ")
        )
    }
}

fn exclusion_path_matches(excluded: &Path, recursive: bool, candidate: &Path) -> bool {
    candidate == excluded || (recursive && candidate.starts_with(excluded))
}

fn absolute_normalized(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
    }
}

/// Resolve the longest existing ancestor, then append the missing tail. This
/// catches an escaping symlink even when the excluded leaf is future output.
// §FS-rhei-plan-language.3.13
fn canonicalize_longest_existing(path: &Path) -> std::io::Result<PathBuf> {
    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() && !existing.is_symlink() {
        let Some(name) = existing.file_name() else {
            break;
        };
        missing.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            break;
        };
        existing = parent;
    }
    let mut resolved = fs::canonicalize(existing)?;
    for part in missing.iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

fn root_for_task<'a>(
    task_roots: &'a HashMap<String, PathBuf>,
    task: &TaskId,
    fallback: &'a Path,
) -> &'a Path {
    task_roots.get(&task.to_string()).map(PathBuf::as_path).unwrap_or(fallback)
}

fn resolve_one_exclusion(
    exclusion: &rhei_core::ast::TaskExclusion,
    citing_task: &TaskId,
    plan_tasks: &[rhei_core::ast::Task],
    task_roots: &HashMap<String, PathBuf>,
    checkout_root: &Path,
    artifact_root: &Path,
) -> Result<ResolvedExclusion, String> {
    use rhei_core::ast::TaskExclusion;
    let (authored, root, relative, recursive) = match exclusion {
        TaskExclusion::Checkout { path, recursive } => {
            (exclusion.authored(), checkout_root, path.trim_end_matches('/').to_string(), *recursive)
        }
        TaskExclusion::Artifact { path, recursive } => {
            (exclusion.authored(), artifact_root, path.trim_end_matches('/').to_string(), *recursive)
        }
        TaskExclusion::Export(export) => {
            let Some(producer) = find_task_by_id(plan_tasks, &export.task) else {
                let shown = local_reference_spelling(citing_task, &export.task);
                return Err(format!(
                    "exclusion '{}' names unknown task '{}'",
                    exclusion.authored(), shown
                ));
            };
            if !producer.provides.iter().any(|name| name == &export.name) {
                return Err(format!(
                    "exclusion '{}' names Task {}, which does not provide '{}'",
                    exclusion.authored(), export.task, export.name
                ));
            }
            let root = root_for_task(task_roots, &export.task, artifact_root);
            (
                exclusion.authored(),
                root,
                task_export_relative_path(&export.task, &export.name),
                false,
            )
        }
    };

    let logical = absolute_normalized(&root.join(&relative));
    let canonical_root = fs::canonicalize(absolute_normalized(root))
        .map_err(|err| format!("cannot resolve {} root '{}': {err}", root_kind(exclusion), root.display()))?;
    let canonical = canonicalize_longest_existing(&logical)
        .map_err(|err| format!("cannot resolve exclusion '{}': {err}", exclusion.authored()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(format!(
            "exclusion '{}' escapes {} root '{}' through its resolved path '{}'",
            exclusion.authored(),
            root_kind(exclusion),
            root.display(),
            canonical.display()
        ));
    }
    Ok(ResolvedExclusion { authored, logical, canonical, recursive })
}

fn local_reference_spelling(citing_task: &TaskId, referenced: &TaskId) -> String {
    let citing = citing_task.to_string();
    let referenced = referenced.to_string();
    let Some((owner, _)) = citing.split_once('.') else {
        return referenced;
    };
    referenced
        .strip_prefix(&format!("{owner}."))
        .unwrap_or(&referenced)
        .to_string()
}

fn root_kind(exclusion: &rhei_core::ast::TaskExclusion) -> &'static str {
    match exclusion {
        rhei_core::ast::TaskExclusion::Checkout { .. } => "checkout",
        _ => "artifact",
    }
}

fn exclusions_overlap(left: &ResolvedExclusion, right: &ResolvedExclusion) -> bool {
    for (left_path, right_path) in [
        (&left.logical, &right.logical),
        (&left.canonical, &right.canonical),
        (&left.logical, &right.canonical),
        (&left.canonical, &right.logical),
    ] {
        if exclusion_path_matches(left_path, left.recursive, right_path)
            || exclusion_path_matches(right_path, right.recursive, left_path)
        {
            return true;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn resolve_task_exclusions(
    task: &rhei_core::ast::Task,
    plan_tasks: &[rhei_core::ast::Task],
    task_roots: &HashMap<String, PathBuf>,
    checkout_root: &Path,
    artifact_root: &Path,
    task_source: &Path,
    machine_source: Option<&Path>,
    machine: &rhei_validator::StateMachine,
) -> Result<ResolvedExclusions, Vec<String>> {
    if task.excludes.is_empty() {
        return Ok(ResolvedExclusions::default());
    }
    let mut errors = Vec::new();
    let mut entries = Vec::new();
    for authored in &task.excludes {
        match resolve_one_exclusion(
            authored,
            &task.id,
            plan_tasks,
            task_roots,
            checkout_root,
            artifact_root,
        ) {
            Ok(entry) => {
                if let Some(previous) = entries.iter().find(|previous| exclusions_overlap(previous, &entry)) {
                    let alias = previous.logical != entry.logical
                        && previous.canonical == entry.canonical;
                    errors.push(if alias {
                        format!(
                            "Task {} has exclusions '{}' and '{}' for the same canonical target '{}'",
                            task.id, previous.authored, entry.authored, entry.canonical.display()
                        )
                    } else {
                        format!(
                            "Task {} has duplicate exclusion '{}' covered by '{}'",
                            task.id, entry.authored, previous.authored
                        )
                    });
                } else {
                    entries.push(entry);
                }
            }
            Err(error) => errors.push(format!("Task {} has invalid **Excludes:**: {error}", task.id)),
        }
    }
    let policy = ResolvedExclusions { entries };

    for consumed in &task.consumes {
        let path = root_for_task(task_roots, &consumed.task, artifact_root)
            .join(task_export_relative_path(&consumed.task, &consumed.name));
        if !policy.allows(&path) {
            let exact_export = task.excludes.iter().any(|entry| {
                matches!(
                    entry,
                    rhei_core::ast::TaskExclusion::Export(export)
                        if export.task == consumed.task && export.name == consumed.name
                )
            });
            errors.push(if exact_export {
                format!(
                    "Task {} both consumes and excludes '{}:{}'; a consumed export is required prompt input",
                    task.id, consumed.task, consumed.name
                )
            } else {
                format!(
                    "Task {} has an exclusion that contains consumed export '{}:{}'; a consumed export is required prompt input",
                    task.id, consumed.task, consumed.name
                )
            });
        }
    }
    if !policy.allows(task_source) {
        errors.push(format!("Task {} excludes its current task source '{}'", task.id, task_source.display()));
    }
    if machine_source.is_some_and(|path| !policy.allows(path)) {
        errors.push(format!(
            "Task {} excludes its active state-machine source '{}'",
            task.id,
            machine_source.unwrap().display()
        ));
    }

    let allowed_states: Vec<&str> = machine
        .profile_for_node(&task.kind, task.profile_level())
        .map(|profile| profile.allowed.iter().map(String::as_str).collect())
        .unwrap_or_else(|| machine.states.keys().map(String::as_str).collect());
    for applicable_state in allowed_states {
        let Some(state) = machine.states.get(applicable_state) else {
            continue;
        };
        for input in state.inputs.iter().filter(|input| !input.optional) {
            let (_, path) = resolve_artifact_path(
                artifact_root,
                input,
                &task.id.to_string(),
                applicable_state,
                Some(1),
                None,
                None,
                None,
                None,
                None,
                None,
            );
            if !policy.allows(&path) {
                errors.push(format!(
                    "Task {} excludes required input '{}' at '{}'",
                    task.id, input.name, input.path
                ));
            }
        }
        if state.snapshot.as_ref().and_then(|snapshot| snapshot.inherit.as_ref()).is_some() {
            errors.push(format!(
                "Task {} uses **Excludes:** in state '{}', but v1 does not combine exclusions with snapshot.inherit",
                task.id, applicable_state
            ));
        }
        if let Some(handoff) = &state.handoff {
            for inherit in handoff.inherit.iter().filter(|inherit| inherit.required) {
                for rule in machine.transitions.iter().filter(|rule| rule.to.0 == applicable_state) {
                    let Some(source) = machine.states.get(&rule.from.0) else { continue };
                    for output in source.outputs.iter().filter(|output| {
                        output.kind.as_deref() == Some("handoff")
                            && inherit.name.as_ref().is_none_or(|name| name == &output.name)
                    }) {
                        let (_, path) = resolve_artifact_path(
                            artifact_root,
                            output,
                            &task.id.to_string(),
                            &rule.from.0,
                            Some(1),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                        );
                        if !policy.allows(&path) {
                            errors.push(format!(
                                "Task {} excludes required handoff '{}' at '{}'",
                                task.id, output.name, output.path
                            ));
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() { Ok(policy) } else { Err(errors) }
}

fn loaded_task_exclusions(
    loaded: &LoadedPlan,
    task: &rhei_core::ast::Task,
    artifact_root: &Path,
    checkout_root: &Path,
    task_source_fallback: &Path,
    machine: &rhei_validator::StateMachine,
    machine_source: Option<&Path>,
) -> Result<ResolvedExclusions, Vec<String>> {
    let task_id = task.id.to_string();
    let task_source = loaded.task_file(&task_id, task_source_fallback);
    resolve_task_exclusions(
        task,
        &loaded.rhei.tasks,
        &loaded.task_roots,
        checkout_root,
        artifact_root,
        &task_source,
        machine_source,
        machine,
    )
}

fn exclusion_report(errors: Vec<String>) -> miette::Report {
    miette!(
        help = "edit **Excludes:** so every entry resolves inside its declared root and does not hide a required source; then run `rhei validate`.",
        "{}",
        errors.join("\n")
    )
}

/// Static pass over every task. The same resolver is called again at dispatch;
/// this pass exists to reject contradictions before an agent is spent.
// §FS-rhei-validate.4 §FS-rhei-plan-language.3.13
fn validate_loaded_exclusions(
    loaded: &LoadedPlan,
    machines: &ResolvedMachineSet,
    input: &Path,
) -> Vec<String> {
    let fallback_root = execution_workspace_root(input);
    let mut errors = Vec::new();
    for task in flatten_task_slice(&loaded.rhei.tasks) {
        if task.excludes.is_empty() {
            continue;
        }
        let task_id = task.id.to_string();
        let artifact_root = loaded.task_root(&task_id, &fallback_root);
        let checkout = resolve_agent_checkout_root(&artifact_root, &task_id)
            .map(|root| root.path)
            .unwrap_or_else(|_| artifact_root.clone());
        let resolved_machine = machines.for_task_str(&task_id);
        if let Err(mut task_errors) = loaded_task_exclusions(
            loaded,
            task,
            &artifact_root,
            &checkout,
            input,
            &resolved_machine.machine,
            resolved_machine.path.as_deref(),
        ) {
            errors.append(&mut task_errors);
        }
    }
    errors
}

/// Apply a declared filesystem-denial adapter to a clone of the selected
/// profile. Extending its fixed command arguments places every pair before the
/// profile separator without changing generic command construction.
// §FS-rhei-agents.1.1.2 §FS-rhei-agents.3
fn agent_with_exclusion_adapter(
    resolved: &ResolvedAgent,
    exclusions: &ResolvedExclusions,
) -> ResolvedAgent {
    let mut adapted = resolved.clone();
    let Some(deny_read) = &adapted.profile.deny_read else {
        return adapted;
    };
    for path in exclusions.adapter_paths() {
        adapted.profile.command.push(deny_read.path_flag.clone());
        adapted.profile.command.push(path.to_string_lossy().into_owned());
    }
    adapted
}

fn prompt_source_allowed(context: &RuntimeTemplateContext<'_>, path: &Path) -> bool {
    context.memory.is_none_or(|memory| memory.exclusions.allows(path))
}
