// `rhei new <title> --under <parent>`: a new ticket inside a rhei, or under an
// existing ticket.
//
// This part decides *what* the ticket is — owning rhei, id, depth, kind,
// state. Where the markdown lands is `new_ticket_write.rs`.

// §FS-rhei-new.3

/// The resolved answer to "under what?", with everything the id allocation
/// needs. §FS-rhei-new.3
struct TicketParent {
    rhei_id: String,
    /// Rhei-local id of the parent ticket; `None` for a top-level ticket.
    parent_local: Option<String>,
    /// Rhei-local ids of the new ticket's existing siblings.
    siblings: Vec<String>,
    /// How to name the parent in an error.
    label: String,
}

fn new_ticket_write(
    target: &Path,
    options: &NewOptions,
    parent: &str,
    description: Option<&str>,
    decision: NewDecision,
) -> MietteResult<NewWrite> {
    reject_malformed_export_flags(options)?;
    // Leniently, so that one unreadable rhei does not take out creates into
    // every other one — `--under basin` most of all. Only the rhei being
    // written to has to load, which is the next check. §FS-rhei-new.5.2
    let loaded = load_plan_leniently(target)?;
    reject_unloadable_target_rhei(&loaded, parent.trim())?;
    reject_unloadable_reference_rheis(&loaded, options)?;
    let placement = resolve_ticket_parent(&loaded, parent.trim())?;
    let entry = resolve_rhei_entry(target, &loaded, &placement.rhei_id)?;
    let structure = rhei_entry_structure(&entry, target)?;

    let segment =
        resolve_new_ticket_segment(options.id.as_deref(), &placement.siblings, &placement.label)?;
    let local_id = match &placement.parent_local {
        Some(parent_local) => format!("{parent_local}.{segment}"),
        None => segment,
    };
    let depth = local_id.split('.').count() as u8;
    reject_excess_depth(&placement, depth, &structure.structure, structure.declared)?;
    let kind =
        resolve_ticket_kind(options.kind.as_deref(), &structure.structure, &placement.rhei_id)?;

    let qualified = format!("{}.{}", placement.rhei_id, local_id);
    let machines = resolve_state_machines_for_create(target, &loaded, &placement.rhei_id)?;
    let machine = machines.machine_for_task_str(&qualified);
    let state = resolve_ticket_state(options.state.as_deref(), machine, &kind, depth)?;

    let block = render_ticket(&TicketFields {
        kind: &kind,
        local_id: &local_id,
        title: &options.title,
        state: &state,
        prior: &options.prior,
        inherits: options.inherits.as_deref(),
        provides: &options.provides,
        consumes: &options.consumes,
        excludes: &options.excludes,
        assignee: options.assignee.as_deref(),
        model: options.model.as_deref(),
        target: options.target.as_deref(),
        description,
    });

    let placed = place_ticket(
        &entry,
        &placement,
        &loaded,
        target,
        &RenderedTicket { local_id: &local_id, title: &options.title, block: &block },
        decision,
    )?;

    Ok(NewWrite {
        kind: "ticket",
        id: qualified,
        title: options.title.trim().to_string(),
        path: placed.path,
        state: Some(state),
        contents: placed.contents,
        preview: block,
        dirs: placed.dirs,
        next_hint: Some("`rhei list` shows the rhei; `rhei next` picks up the work".to_string()),
        notes: ticket_create_notes(options),
    })
}

/// What the flags do that they do not say on their face.
///
/// `--assignee` is the whole list: an assignee reads as a label to whoever is
/// writing the plan and as "claimed, in progress" to the engine, so a plan
/// authored with assignees is one `rhei run` will not start, and `rhei next`
/// and `rhei list --ready` then disagree about the same ticket. Authoring a
/// claimed ticket is legitimate, so this is a note and not a refusal.
// §FS-rhei-new.5.4
fn ticket_create_notes(options: &NewOptions) -> Vec<String> {
    let Some(assignee) = options.assignee.as_deref() else {
        return Vec::new();
    };
    vec![format!(
        "`--assignee {}` marks the ticket claimed and in progress: `rhei next` and `rhei run` \
         skip it until `rhei release <id>`.",
        assignee.trim()
    )]
}

/// Refuse when the rhei this ticket is going into is the one that will not
/// load.
///
/// That failure *is* this create's business: the rhei's existing ids decide the
/// new one's number and its `## Tasks` section decides where the block goes,
/// and a lenient load has neither. Every other rhei's parse error is left to
/// the pre/post diff.
// §FS-rhei-new.5.2
fn reject_unloadable_target_rhei(loaded: &LoadedPlan, parent: &str) -> MietteResult<()> {
    let rhei_id = parent.split('.').next().unwrap_or(parent);
    let marker = format!("rhei '{rhei_id}' could not be loaded");
    let Some(skipped) = loaded.unloadable.iter().find(|message| message.starts_with(&marker))
    else {
        return Ok(());
    };
    Err(miette!(
help = "fix that rhei and re-run; `rhei validate` reports it with a code frame. Creates into every other rhei in this project are unaffected.",

        "{skipped}\n\n`rhei new` cannot place a ticket in a rhei it cannot read: the ids already \
         in it decide the new ticket's number, and its `## Tasks` section decides where the \
         block goes."
    ))
}

/// Resolve the project's state machines the way every command does, except that
/// a rhei this create is not writing to keeps its unresolvable machine to
/// itself.
///
/// One rhei declaring `**States:** billing-review` with no `states.yaml` to
/// match is a mid-edit state someone is *in the middle of leaving* — the
/// declaration is written, the machine is not yet. Resolved strictly, it stops
/// every create in the project, basin capture included — and basin capture is
/// the one thing that has to survive somebody else's half-finished work. The
/// rhei being written to is different: the new ticket's starting state comes
/// out of its machine, so that one must resolve.
///
/// The project still fails validation for it — both passes, identically — so
/// the create keeps its write and says the failure is not its own. Nothing is
/// hidden; it is only not this create's to refuse.
// §FS-rhei-new.5.2 §AR-rhei-panta.4
fn resolve_state_machines_for_create(
    input: &Path,
    loaded: &LoadedPlan,
    target_rhei: &str,
) -> MietteResult<ResolvedMachineSet> {
    let default = resolve_state_machine_for_loaded_plan(input, loaded, None)?;
    let mut per_rhei = BTreeMap::new();
    let mut declared: Vec<(&String, &String)> = loaded.rhei_machines.iter().collect();
    declared.sort();
    for (rhei_id, machine_name) in declared {
        // Creation preserves explicit declaration provenance when selecting
        // both the initial state and an explicit state. §FS-rhei-plan-language.1.3
        match resolve_declared_rhei_machine(input, loaded, rhei_id, machine_name, &default) {
            Ok(resolved) => {
                per_rhei.insert(rhei_id.clone(), resolved);
            }
            Err(report) if rhei_id == target_rhei => return Err(report),
            Err(_) => {}
        }
    }
    Ok(ResolvedMachineSet { default, per_rhei })
}

/// Refuse a `**Prior:**` or `**Consumes:**` that points into a rhei the lenient
/// load skipped.
///
/// The target rhei is not the only one this create depends on being readable:
/// a reference into a rhei that will not parse cannot be checked by anything.
/// Written anyway, it passes the pre/post diff — the reference resolves against
/// no ticket either side of the write — and the create then says the errors are
/// not its own, which is exactly backwards: the error that shows up the moment
/// the sibling is repaired is this create's `--prior`, written into a file
/// nobody has looked at since.
// §FS-rhei-new.5.2 §FS-rhei-new.3.3
fn reject_unloadable_reference_rheis(
    loaded: &LoadedPlan,
    options: &NewOptions,
) -> MietteResult<()> {
    let priors = options.prior.iter().map(|value| ("--prior", value.as_str()));
    let consumed = options
        .consumes
        .iter()
        .filter_map(|value| value.split_once(':').map(|(task, _)| ("--consumes", task)));
    let excluded = options.excludes.iter().filter_map(|value| {
        (!value.starts_with("checkout=") && !value.starts_with("artifact="))
            .then(|| value.split_once(':').map(|(task, _)| ("--excludes", task)))
            .flatten()
    });
    for (flag, reference) in priors.chain(consumed).chain(excluded) {
        let Some(rhei_id) = referenced_rhei_id(reference) else {
            continue;
        };
        let marker = format!("rhei '{rhei_id}' could not be loaded");
        let Some(skipped) = loaded.unloadable.iter().find(|message| message.starts_with(&marker))
        else {
            continue;
        };
        return Err(miette!(
help = "fix that rhei and re-run; `rhei validate` reports it with a code frame. Drop the reference to create the ticket now and add it once the rhei reads.",

            "{skipped}\n\n{flag} '{}' points into rhei '{rhei_id}', so nothing can check that \
             the reference resolves — and a reference written unchecked comes back as an error \
             the moment that rhei is repaired.",
            reference.trim()
        ));
    }
    Ok(())
}

/// The rhei a reference names, when it names one.
///
/// `**Prior:**` values carry an optional node-kind keyword (`Task 3`), so the
/// id is the last whitespace-separated token; only a dotted id crosses a rhei
/// boundary, and a bare `3` is local to the rhei being written to.
// §FS-rhei-plan-language.3.1 §AR-rhei-panta.3
fn referenced_rhei_id(reference: &str) -> Option<&str> {
    let id = reference.trim().rsplit(char::is_whitespace).next()?;
    let (leading, _) = id.split_once('.')?;
    (!leading.is_empty()).then_some(leading)
}

/// Check the two reference flags for shape before the write, so a mistyped
/// `--consumes auth.1` is a message about the flag rather than a parse error
/// with a line number in a file the write just produced.
///
/// Shape only: whether the reference resolves to a declared `**Provides:**` is
/// a question nothing answers yet.
// §FS-rhei-new.3.3 §FS-rhei-plan-language.3.12 §FS-rhei-new.6
fn reject_malformed_export_flags(options: &NewOptions) -> MietteResult<()> {
    for name in &options.provides {
        let name = name.trim();
        if name.is_empty() || !is_legal_export_name(name) {
            return Err(miette!(
help = "an export name is one word: letters, digits, '.', '_', or '-'. Repeat --provides, or separate several with commas.",

                "--provides '{name}' is not a valid export name: it starts with a letter or a \
                 digit and continues with letters, digits, '.', '_', or '-' \
                 (`--provides api-contract`)"
            ));
        }
    }
    for reference in &options.consumes {
        let reference = reference.trim();
        let shape_ok = reference
            .split_once(':')
            .is_some_and(|(task, name)| is_legal_task_id(task) && is_legal_export_name(name));
        if !shape_ok {
            return Err(miette!(
help = "name the ticket and the export it publishes, separated by a colon. Repeat --consumes, or separate several with commas.",

                "--consumes '{reference}' is not a valid reference: it is \
                 '<task-id>:<export-name>' (`--consumes auth.1:api-contract`)"
            ));
        }
    }
    // Parse exclusions through the language parser so `rhei new` and an
    // edited plan accept exactly the same portable grammar. §FS-rhei-new.1.3
    for exclusion in &options.excludes {
        let probe = format!(
            "# Rhei: Probe\n\n## Tasks\n\n### Task 1: Probe\n**State:** pending\n**Excludes:** {exclusion}\n"
        );
        if let Err(error) = rhei_core::parse(&probe) {
            return Err(miette!(
                help = "use checkout=<relative-path>, artifact=<relative-path>, or <task-id>:<export-name>; repeat --excludes for several entries.",
                "--excludes '{}' is not valid: {}",
                exclusion.trim(),
                error.message
            ));
        }
    }
    Ok(())
}

/// Read `--under`: a single segment names the owning rhei, anything dotted
/// names a parent ticket. Ticket ids are project-qualified, so they always
/// carry at least two segments — the two forms can never collide.
// §FS-rhei-new.3 §AR-rhei-panta.3
fn resolve_ticket_parent(loaded: &LoadedPlan, parent: &str) -> MietteResult<TicketParent> {
    if parent.is_empty() {
        return Err(miette!(
help = "name a rhei (`--under auth`) or a ticket (`--under auth.3`).",
            "--under needs a rhei id or a ticket id"
        ));
    }
    if !parent.contains('.') {
        let known = loaded.rhei_ids.iter().any(|id| id == parent);
        // The basin is created on demand, so it is a legal parent before it
        // exists. §FS-rhei-panta.2
        if !known && parent != workspace::BASIN_RHEI_ID {
            return Err(unknown_parent_error(loaded, parent));
        }
        let prefix = format!("{parent}.");
        let siblings = loaded
            .rhei
            .tasks
            .iter()
            .filter_map(|task| task.id.to_string().strip_prefix(&prefix).map(ToOwned::to_owned))
            .filter(|local| !local.contains('.'))
            .collect();
        return Ok(TicketParent {
            rhei_id: parent.to_string(),
            parent_local: None,
            siblings,
            label: format!("rhei '{parent}'"),
        });
    }

    let Some(task) = find_task_by_id_str(&loaded.rhei.tasks, parent) else {
        return Err(unknown_parent_error(loaded, parent));
    };
    let (rhei_id, parent_local) = parent
        .split_once('.')
        .map(|(rhei, local)| (rhei.to_string(), local.to_string()))
        .expect("a dotted id splits");
    let siblings = task
        .children
        .iter()
        .filter_map(|child| {
            child.id.to_string().rsplit_once('.').map(|(_, last)| last.to_string())
        })
        .collect();
    Ok(TicketParent {
        rhei_id,
        parent_local: Some(parent_local),
        siblings,
        label: format!("ticket {parent}"),
    })
}

fn unknown_parent_error(loaded: &LoadedPlan, parent: &str) -> miette::Report {
    let mut known: Vec<String> = loaded.rhei_ids.clone();
    if !known.iter().any(|id| id == workspace::BASIN_RHEI_ID) {
        known.push(workspace::BASIN_RHEI_ID.to_string());
    }
    miette!(
        help = did_you_mean(parent, &known)
            .unwrap_or_else(|| "pass a rhei id, or a ticket id to nest under.".to_string()),
        "'{parent}' names no rhei or ticket in this project. --under takes a rhei id \
         ({}) for a top-level ticket, or a ticket id like `{}.1` for a subtask",
        known.join(", "),
        known.first().map(String::as_str).unwrap_or("auth")
    )
}

/// Refuse a subtask deeper than the rhei allows, before anything is written.
// §FS-rhei-new.3.3 §FS-rhei-plan-language.3.4
fn reject_excess_depth(
    placement: &TicketParent,
    depth: u8,
    structure: &rhei_core::ast::Structure,
    declared_max_levels: bool,
) -> MietteResult<()> {
    let max_levels = structure.max_levels;
    if depth <= max_levels {
        return Ok(());
    }
    // A rhei created without `--max-levels` has no frontmatter block at all, so
    // "raise it in the frontmatter" names a field that is not there. Spell out
    // the block instead. §FS-rhei-new.3.3
    let help = if declared_max_levels {
        format!(
            "raise `structure.maxLevels` to {depth} in the rhei's frontmatter, or add the \
             ticket higher up."
        )
    } else {
        format!(
            "this rhei declares no frontmatter, so {max_levels} is the default. Add a block \
             right under the `# Rhei:` heading — a line `---`, then `structure:`, then \
             `  maxLevels: {depth}`, then `---` — or add the ticket higher up."
        )
    };
    Err(miette!(
        help = help,
        "a ticket under {} would sit at depth {depth}, but rhei '{}' allows {max_levels} \
         (`structure.maxLevels`)",
        placement.label,
        placement.rhei_id
    ))
}

/// The heading keyword, checked against what the rhei declares.
// §FS-rhei-new.3.3 §FS-rhei-plan-language.3.7
fn resolve_ticket_kind(
    requested: Option<&str>,
    structure: &rhei_core::ast::Structure,
    rhei_id: &str,
) -> MietteResult<String> {
    let kind = requested.unwrap_or("task").trim().to_ascii_lowercase();
    if structure.node_kinds.iter().any(|declared| declared.eq_ignore_ascii_case(&kind)) {
        return Ok(kind);
    }
    let declared = structure.node_kinds.join(", ");
    // A rhei that does not declare `task` makes `--kind` mandatory. Reporting
    // the default back as though the user had typed it blames them for a word
    // that came from the command. §FS-rhei-new.3.3
    let Some(requested) = requested else {
        return Err(miette!(
            help = format!(
                "add the flag, for example: --kind {}",
                structure.node_kinds.first().map(String::as_str).unwrap_or("task")
            ),
            "rhei '{rhei_id}' requires --kind: it declares {declared}, and no default among \
             them"
        ));
    };
    Err(miette!(
        help = did_you_mean(&kind, &structure.node_kinds)
            .unwrap_or_else(|| "declare the kind in `structure.nodeKinds` first.".to_string()),
        "rhei '{rhei_id}' does not declare the node kind '{}'. It declares: {declared}",
        requested.trim()
    ))
}

/// The state the ticket is created in: the machine's initial state for this
/// node, or the one `--state` names, checked against that same machine.
// §FS-rhei-new.3.2
fn resolve_ticket_state(
    requested: Option<&str>,
    machine: &rhei_validator::StateMachine,
    kind: &str,
    depth: u8,
) -> MietteResult<String> {
    let Some(requested) = requested else {
        return initial_state_for_node(machine, kind, depth);
    };
    let normalized = normalized_state_name(requested.trim(), machine);
    if machine.is_valid_state(&normalized) {
        return Ok(normalized);
    }
    let known: Vec<String> = machine.allowed_states().map(ToOwned::to_owned).collect();
    Err(miette!(
        help = did_you_mean(requested.trim(), &known)
            .unwrap_or_else(|| "omit --state to start in the machine's initial state.".to_string()),
        "state machine '{}' has no state '{}'. It declares: {}",
        machine.name,
        requested.trim(),
        known.join(", ")
    ))
}
