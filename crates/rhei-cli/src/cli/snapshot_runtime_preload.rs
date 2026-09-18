
/// The two roots a snapshot preload resolves against: `project` is the project
/// workspace root the shared snapshot cache lives under, `execution` the owning
/// rhei's execution root the per-ticket session directory lives under. One
/// directory in a single-file layout, two in a Panta project — which is when
/// passing either for the other goes unnoticed.
/// §FS-rhei-snapshots.7 §FS-rhei-panta.6.4
#[derive(Clone, Copy)]
struct SnapshotPreloadRoots<'a> {
    project: &'a Path,
    execution: &'a Path,
}

/// Overlay the task's authored control onto the active state's rule.
///
/// The task form replaces only name/axis; all selectors and policy fields are
/// retained. An explicit `none` removes the effective contract altogether.
// §FS-rhei-snapshots.4.2 §FS-rhei-plan-language.3.14
fn effective_snapshot_inherit(
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    current_state: &str,
) -> Option<rhei_validator::SnapshotInheritConfig> {
    use rhei_core::ast::TaskSnapshotInherit;

    let state_rule = machine
        .states
        .get(current_state)
        .and_then(|state| state.snapshot.as_ref())
        .and_then(|snapshot| snapshot.inherit.as_ref())
        .cloned();
    match task.inherits.as_ref() {
        None => state_rule,
        Some(TaskSnapshotInherit::Disabled) => None,
        Some(TaskSnapshotInherit::Rule { name, from_axis }) => {
            let mut effective = state_rule.unwrap_or(rhei_validator::SnapshotInheritConfig {
                name: name.clone(),
                from_axis: Some(from_axis.clone()),
                compat: None,
                required: None,
                select: None,
            });
            effective.name.clone_from(name);
            effective.from_axis = Some(from_axis.clone());
            Some(effective)
        }
    }
}

/// Terminal, non-cancelled declared predecessors are the only tasks a Prior
/// lookup may inspect. Each source is judged by its owning rhei's machine.
// §FS-rhei-snapshots.4.3 §FS-rhei-panta.6.1
fn eligible_prior_snapshot_sources(
    task: &rhei_core::ast::Task,
    plan_tasks: &[rhei_core::ast::Task],
    machines: &ExecutionMachines,
) -> Vec<String> {
    task.prior
        .iter()
        .filter_map(|prior| {
            let source = find_task_by_id(plan_tasks, prior)?;
            let machine = machines.for_task(&source.id);
            let state = normalized_state_name(&source.state, machine);
            (is_terminal_state(&state, machine)
                && !rhei_validator::is_cancelled_state_name(&state))
            .then(|| source.id.to_string())
        })
        .collect()
}

/// Orchestration hook for snapshot preload, invoked before spawning an agent
/// for a state that declares named `snapshot.inherit` or state-local
/// `session: continue`.
///
/// Per the run execution loop and snapshot preload contract, the orchestrator
/// Named inheritance honors its explicit overrides and strictness. State-local
/// continuation selects only the preceding visit's current auto snapshot and
/// degrades cold. Both apply the agent's resume/fork strategy and stage the
/// session before the subprocess starts.
// §FS-rhei-run.3 §FS-rhei-snapshots.10.1: Preload before spawning.
///
/// The actual preload is owned by the impl-rhei-snapshots task; this hook
/// pins the call site so the orchestration ordering is encoded in code, not
/// just in the spec text. Once impl-rhei-snapshots delivers the snapshot
/// module, the body of this function calls into that module and may return a
/// `missing-snapshot`, `incompatible-snapshot`, or
/// `unsupported-snapshot-session` error.
// §FS-rhei-snapshots.10.1: Snapshot preload errors.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
fn preload_snapshot_inherit_before_spawn(
    input: &Path,
    roots: SnapshotPreloadRoots<'_>,
    spawn_working_dir: &Path,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    current_state: &str,
    resolved: &ResolvedAgent,
    settings: &RheiSettings,
    visit_count: u64,
    override_selection: Option<&SnapshotOverrideRunSelection>,
    opts: &RunOptions,
) -> MietteResult<SnapshotPreload> {
    preload_snapshot_inherit_before_spawn_with_prior_sources(
        input,
        roots,
        spawn_working_dir,
        machine,
        task,
        current_state,
        resolved,
        settings,
        visit_count,
        override_selection,
        opts,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn preload_snapshot_inherit_before_spawn_with_prior_sources(
    input: &Path,
    roots: SnapshotPreloadRoots<'_>,
    spawn_working_dir: &Path,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    current_state: &str,
    resolved: &ResolvedAgent,
    settings: &RheiSettings,
    visit_count: u64,
    override_selection: Option<&SnapshotOverrideRunSelection>,
    opts: &RunOptions,
    prior_sources: &[String],
) -> MietteResult<SnapshotPreload> {
    let mut preload = SnapshotPreload::default();
    let effective_inherit = effective_snapshot_inherit(machine, task, current_state);
    let declares_inherit = effective_inherit.is_some();
    let continues = machine.states.get(current_state).and_then(|state| state.session)
        == Some(rhei_validator::StateSession::Continue);

    let target_slug = if declares_inherit || continues {
        Some(snapshot_target_slug_or_err(resolved)?)
    } else {
        resolved_agent_target_slug(resolved)
    };
    let Some(target_slug) = target_slug else {
        return Ok(preload);
    };
    let override_applies = take_snapshot_override_for_invocation(
        override_selection,
        task,
        &target_slug,
        opts.snapshot_override_ref().is_some(),
    );
    if override_applies && !declares_inherit {
        if matches!(
            task.inherits.as_ref(),
            Some(rhei_core::ast::TaskSnapshotInherit::Disabled)
        ) {
            return Err(miette!(
                help = snapshot_help(),
                "--from-snapshot has no effective snapshot inheritance contract because Task {} declares **Inherits:** none; --override-inherit does not bypass the opt-out",
                task.id
            ));
        }
        return Err(miette!(
            help = snapshot_help(),
            "--from-snapshot requires the target state '{}' to declare snapshot.inherit; --override-inherit does not bypass that authored contract",
            current_state
        ));
    }
    if let Some(session) = snapshot_session(resolved) {
        if let Some(flag) = snapshot_session_string(session, "session_dir_flag") {
            // The session belongs to the rhei that ran the ticket, not to the
            // project: `rhei reset --rhei` sweeps it there. §FS-rhei-snapshots.7
            let dir = snapshot_session_dir(
                roots.execution,
                &task.id.to_string(),
                current_state,
                &target_slug,
            );
            fs::create_dir_all(&dir).map_err(|err| {
                file_io_report(&dir, "failed to create snapshot session dir", err)
            })?;
            preload.extra_args.push(flag);
            preload.extra_args.push(dir.display().to_string());
            preload.session_dir = Some(dir);
        } else if let Some((layout, template)) = snapshot_session_layout(session)
            .and_then(|layout| snapshot_layout_dir_template(layout).map(|t| (layout, t)))
        {
            // §FS-rhei-snapshots.9.1 §FS-rhei-snapshots.10.1: Fixed-location emit,
            // with the §9.1.1 locator keys. Read-only after exit; a resolution
            // failure degrades to no tracking rather than failing the spawn.
            match resolve_snapshot_dir_template(&template, spawn_working_dir) {
                Ok(dir) => match resolve_snapshot_session_locator(layout, spawn_working_dir) {
                    Ok(locator) => {
                        preload.fixed_session_scan_floor = Some(std::time::SystemTime::now());
                        if let Some(flag) = snapshot_session_string(session, "assign_id_flag") {
                            let id = generate_snapshot_session_id();
                            preload.extra_args.push(flag);
                            preload.extra_args.push(id.clone());
                            preload.fixed_session_id = Some(id);
                        }
                        preload.fixed_session_dir = Some(dir);
                        preload.fixed_session_locator = locator;
                    }
                    // A malformed locator key takes the same degrade path: this
                    // invocation loses emit for that agent, not the run.
                    // §FS-rhei-snapshots.9.1.1
                    Err(err) => {
                        diag_warn!(
                            "warning: could not resolve snapshot session locator keys for agent '{}' ({}); fixed-location snapshot tracking disabled for this spawn",
                            resolved.agent.id(),
                            err
                        );
                    }
                },
                Err(err) => {
                    diag_warn!(
                        "warning: could not resolve snapshot dir_template for agent '{}' ({}); fixed-location snapshot tracking disabled for this spawn",
                        resolved.agent.id(),
                        err
                    );
                }
            }
        }
    }

    if continues {
        // State-local continuation is an implicit, optional source. It never
        // broadens `--from-snapshot` or the named-inherit grammar.
        // §FS-rhei-snapshots.4.7 §FS-rhei-snapshots.10.1
        return preload_state_session_continuation(
            preload,
            roots.project,
            task,
            current_state,
            resolved,
            settings,
            visit_count,
            &target_slug,
        );
    }

    let Some(inherit) = effective_inherit.as_ref() else {
        return Ok(preload);
    };
    let required = inherit.required.unwrap_or(false);
    let compat = inherit.compat.as_deref().unwrap_or("native");
    if override_applies && compat == "none" && !opts.override_inherit() {
        return Err(miette!(
            help = snapshot_help(),
            "--from-snapshot cannot override snapshot.inherit '{}' because compat: none disables authored preload; pass --override-inherit to bypass compatibility checks",
            inherit.name
        ));
    }
    if compat == "none" && !opts.override_inherit() {
        diag_warn!("info: snapshot preload disabled by compat: none");
        return Ok(preload);
    }

    // The project root, so inherit reads the cache emit wrote — an execution
    // root would be a cache nothing writes to. §FS-rhei-snapshots.7
    let cache_root = snapshot_cache_dir(settings, roots.project);
    let source = if override_applies {
        let reference = opts.snapshot_override_ref().ok_or_else(|| {
            miette!(
                help = internal_error_help(),
                "internal error: snapshot override selected without a reference"
            )
        })?;
        let loaded = load_plan(input)?;
        let ctx = SnapshotCommandContext {
            workspace_root: roots.project.to_path_buf(),
            plan_path: input.to_path_buf(),
            cache_root: cache_root.clone(),
            loaded,
            // Single-ticket context: the preloading ticket's machine stands
            // in for the set. §DA-per-rhei-state-machines
            machines: rhei_validator::MachineSet::single(machine.clone()),
            settings: settings.clone(),
        };
        // `--target` has already selected the inheriting invocation. Source
        // snapshot target constraints come from the reference and inherit
        // contract, not from the run invocation selector.
        let record = resolve_snapshot_ref(&ctx, reference, None, None)?;
        if !opts.override_inherit() {
            validate_snapshot_override_contract(
                &cache_root,
                task,
                current_state,
                inherit,
                &target_slug,
                visit_count,
                prior_sources,
                &record,
                resolved,
            )?;
        }
        Some(record)
    } else {
        resolve_inherit_snapshot_source_with_prior(
            &cache_root,
            task,
            current_state,
            inherit,
            &target_slug,
            visit_count,
            prior_sources,
        )?
    };

    let Some(source) = source else {
        if required {
            return Err(miette!(
                help = "no cached snapshot matches that inherit selector. List what is cached with: rhei snapshot list, or run the producing state first.",
                "missing-snapshot: no snapshot found for inherit '{}'",
                inherit.name
            ));
        }
        diag_warn!("warning: no snapshot found for inherit: {}; running cold", inherit.name);
        return Ok(preload);
    };
    if source.completion == "timeout" && !opts.override_inherit() {
        if required {
            return Err(miette!(
                help = snapshot_resume_help(),
                "incompatible-snapshot: selected snapshot {} completed by timeout and is not preloadable",
                source.display_ref()
            ));
        }
        diag_warn!(
            "warning: timed-out snapshot {} is not preloadable; running cold",
            source.display_ref()
        );
        return Ok(preload);
    }
    if compat == "native"
        && !opts.override_inherit()
        && !snapshot_record_native_compatible(&source, resolved)
    {
        if required {
            return Err(miette!(
                help = snapshot_resume_help(),
                "incompatible-snapshot: selected snapshot {} is not native-compatible with agent '{}'",
                source.display_ref(),
                resolved.agent.id()
            ));
        }
        diag_warn!(
            "warning: preload skipped: incompatible agent for {}; running cold",
            source.display_ref()
        );
        return Ok(preload);
    }
    let Some(session) = snapshot_session(resolved) else {
        if required {
            return Err(miette!(
                help = snapshot_help(),
                "unsupported-snapshot-session: agent '{}' has no supported snapshot preload strategy",
                resolved.agent.id()
            ));
        }
        diag_warn!(
            "warning: unsupported-snapshot-session: agent '{}' has no supported snapshot preload strategy; running cold",
            resolved.agent.id()
        );
        return Ok(preload);
    };
    if !snapshot_preload_session_supported(session) {
        if required {
            return Err(miette!(
                help = snapshot_help(),
                "unsupported-snapshot-session: agent '{}' has no supported snapshot preload strategy",
                resolved.agent.id()
            ));
        }
        diag_warn!(
            "warning: unsupported-snapshot-session: agent '{}' has no supported snapshot preload strategy; running cold",
            resolved.agent.id()
        );
        return Ok(preload);
    }
    if let Some(reason) = snapshot_cache_benefit_reason(&source, resolved) {
        diag_warn!(
            "info: snapshot {} is native-compatible but may not be cache-beneficial: {}",
            source.display_ref(),
            reason
        );
    }
    if let Some(flag) = snapshot_strategy_flag(session, "fork") {
        preload.extra_args.push(flag);
        preload.extra_args.push(source.transcript_path().display().to_string());
    } else if let Some(flag) = snapshot_strategy_flag(session, "resume") {
        let session_id = source
            .manifest
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        preload.extra_args.push(flag);
        preload.extra_args.push(session_id.to_string());
    }
    if let Some(session_dir) = preload.session_dir.as_ref() {
        let ext = source
            .manifest
            .get("session_layout")
            .and_then(snapshot_layout_ext)
            .unwrap_or_else(|| "jsonl".to_string());
        let target = session_dir.join(format!(
            "{}.{}",
            source
                .manifest
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("source"),
            ext
        ));
        fs::copy(source.transcript_path(), &target).map_err(|err| {
            file_io_report(&target, "failed to stage snapshot transcript for preload", err)
        })?;
    }
    preload.parent_ref = Some(snapshot_parent_ref(&source));
    Ok(preload)
}
