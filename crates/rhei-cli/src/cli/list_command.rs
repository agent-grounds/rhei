// `rhei list` — the filter set, the walk it does over a loaded plan, and what
// it prints.
//
// Its own part because listing is read-only presentation: it decides nothing
// about state, resolves no target, and writes no file.

// §FS-rhei-list

/// Filter set for the `list` subcommand. See `Commands::List` for flag docs.
struct ListFilters {
    /// Narrow to named rheis; empty is the whole project. §FS-rhei-panta.6.4
    rhei: Vec<String>,
    states: Vec<String>,
    assignee: Option<String>,
    no_assignee: bool,
    kind: Option<String>,
    has_prior: Option<String>,
    parent: Option<String>,
    root: bool,
    contains: Option<String>,
    terminal: bool,
    non_terminal: bool,
    ready: bool,
    blocked: bool,
    limit: usize,
}

impl ListFilters {
    /// True when nothing narrows the listing, so it is showing the plan rather
    /// than answering a question about part of it. §FS-rhei-list.4.1
    fn none_active(&self) -> bool {
        self.rhei.is_empty()
            && self.states.is_empty()
            && self.assignee.is_none()
            && !self.no_assignee
            && self.kind.is_none()
            && self.has_prior.is_none()
            && self.parent.is_none()
            && !self.root
            && self.contains.is_none()
            && !self.terminal
            && !self.non_terminal
            && !self.ready
            && !self.blocked
            && self.limit == 0
    }
}

/// Name the rheis that hold no tickets, in the wording
/// `rhei render --format progress` already uses for them.
///
/// `rhei init` ends by pointing at `rhei new "<title>"`, which makes the very
/// next `rhei list` the moment a project holds one rhei and no tickets — and a
/// listing that showed nothing would read as though the create had not landed.
/// Text only, and only unfiltered: a filter asks a question about tickets, and
/// a rhei with none has no answer to give.
// §FS-rhei-list.4.1 §FS-rhei-new.5.4
fn report_empty_rheis(loaded: &LoadedPlan) {
    if !loaded.is_panta_project() {
        return;
    }
    for id in &loaded.rhei_ids {
        if rhei_holds_tickets(loaded, id) {
            continue;
        }
        println!();
        println!("{}: (no tickets yet)", empty_rhei_heading(loaded, id));
    }
}

/// True when any ticket in the merged graph belongs to rhei `id`.
fn rhei_holds_tickets(loaded: &LoadedPlan, id: &str) -> bool {
    let prefix = format!("{id}.");
    loaded.rhei.tasks.iter().any(|task| task.id.to_string().starts_with(&prefix))
}

/// What to print when nothing matched: the rheis `--rhei` named, when every one
/// of them is simply empty, and the generic filter line otherwise.
///
/// `--rhei` is a filter, so an empty rhei falls into "no tasks match the given
/// filters" — which reads as "your filters are wrong" about a rhei that has no
/// tickets for any filter to match. The distinction matters right after a
/// create, when the rhei someone just made is exactly the one they are listing.
// §FS-rhei-list.4.1 §FS-rhei-new.5.4
fn empty_listing_line(loaded: &LoadedPlan, filters: &ListFilters) -> String {
    let named: Vec<&str> = filters.rhei.iter().map(String::as_str).collect();
    if named.is_empty() || named.iter().any(|id| rhei_holds_tickets(loaded, id)) {
        return "(no tasks match the given filters)".to_string();
    }
    let quoted: Vec<String> = named.iter().map(|id| format!("'{id}'")).collect();
    match quoted.len() {
        1 => format!("(rhei {} holds no tickets yet)", quoted[0]),
        _ => format!("(rheis {} hold no tickets yet)", quoted.join(", ")),
    }
}

/// `Billing (billing)` — the rhei's title, carrying the id when the two
/// diverge, exactly as `rhei render --format progress` heads its blocks. The
/// merge emits one `Rhei <id>: <title>` section per rhei, which is where the
/// title comes from.
// §FS-rhei-render.3.4 §FS-rhei-panta.4
fn empty_rhei_heading(loaded: &LoadedPlan, id: &str) -> String {
    let marker = format!("Rhei {id}: ");
    let title = loaded
        .rhei
        .content_sections
        .iter()
        .filter(|section| section.rhei.as_deref() == Some(id))
        .find_map(|section| section.title.strip_prefix(&marker))
        .unwrap_or(id);
    if title.eq_ignore_ascii_case(id) {
        title.to_string()
    } else {
        format!("{title} ({id})")
    }
}

/// The person a ticket is waiting on, when its current state is a poll that
/// declares one.
///
/// `list` is the surface an operator checks first, and a self-resuming poll
/// that waits for a reply is otherwise indistinguishable there from one
/// watching a build — the same row, the same state, no reading of whose turn
/// it is. §FS-rhei-states.2.5 §FS-rhei-list.4.1
fn task_waiting_on_person<'a>(
    task: &rhei_core::ast::Task,
    machines: &'a rhei_validator::MachineSet,
) -> Option<&'a str> {
    let machine = machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    machine.states.get(&state)?.waiting_on_person()
}

/// Execute the `list` subcommand: load a plan and print tasks matching the
/// provided filters. Modeled after `bd list` from beads, with a filter set
/// adapted to Rhei's data model (no priority/labels/timestamps).
fn list_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    filters: ListFilters,
    as_json: bool,
) -> MietteResult<()> {
    // Listing is the surface an author reaches for *while* a plan is broken, so
    // it reports what it could not load and shows the rest. §FS-rhei-panta.6
    let loaded = load_plan_leniently(input)?;
    for skipped in &loaded.unloadable {
        eprintln!("warning: {skipped}");
    }
    let rhei_scope = resolve_rhei_scope(&loaded, &filters.rhei)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = resolved.validator_set();

    // The `metadata` field's source, converted once for the whole document by
    // the conversion `render --format json` uses, so neither a stored value nor
    // a value JSON cannot hold can read differently on the two surfaces.

    // A fault in the plan is not a fault in the query, so it is reported whether
    // or not the filters kept the ticket that holds it.
    // §FS-rhei-list.4.2 §FS-rhei-render.3.1.1
    let frontmatter = if as_json {
        loaded
            .rhei
            .metadata
            .as_ref()
            .map(frontmatter_to_json)
            .transpose()
            .map_err(|found| unrepresentable_frontmatter_error(input, &found))?
    } else {
        None
    };

    // Flatten the task tree into (task, parent_id) pairs, preserving source order.
    let mut flat: Vec<(&rhei_core::ast::Task, Option<TaskId>)> = Vec::new();
    fn walk<'a>(
        task: &'a rhei_core::ast::Task,
        parent: Option<TaskId>,
        out: &mut Vec<(&'a rhei_core::ast::Task, Option<TaskId>)>,
    ) {
        out.push((task, parent));
        let parent_id = Some(task.id.clone());
        for child in &task.children {
            walk(child, parent_id.clone(), out);
        }
    }
    for task in &loaded.rhei.tasks {
        walk(task, None, &mut flat);
    }

    // §FS-rhei-panta.6: an empty project is a valid project, not an error —
    // say what it is and how to grow it.
    if flat.is_empty() {
        if as_json {
            println!("[]");
        } else if loaded.is_panta_project() {
            println!("(project has no tickets yet)");
            // A project that already holds a rhei needs its rheis named, not a
            // second invitation to add one — that is the state `rhei init`
            // plus one `rhei new` leaves behind. §FS-rhei-list.4.1
            if loaded.rhei_ids.is_empty() {
                println!("{}", add_a_rhei_help());
            } else if filters.none_active() {
                report_empty_rheis(&loaded);
            }
        } else {
            println!("(this rhei has no tickets yet)");
        }
        return Ok(());
    }

    // A listing that re-derived readiness drifted from the definition of it: it
    // judged terminality, gating, priors and the supervision barrier, and never
    // looked at the state's required `inputs:` or at a `poll:` deadline still
    // ahead — so it named work both `rhei next` and `rhei run` refused, on the
    // surface an operator checks first. Asking for the set is the only shape in
    // which the three cannot disagree about readiness.

    // §FS-rhei-list.3.1 §FS-rhei-next.3: `--ready` is the ready set itself —
    // the set `rhei next` draws from, not the narrower one it claims out of, so
    // an assigned or mid-workflow ticket is listed here and refused there.

    // Whole-plan, and narrowed afterwards by the row filters exactly as
    // `next_command` narrows after `find_claimable_tasks`. §FS-rhei-panta.6.1

    // Built once, and only when a flag asks: this is the only filesystem read
    // `list` does — the scan stats each required input artifact.
    let ready_ids: HashSet<&TaskId> = if filters.ready || filters.blocked {
        let workspace_root = execution_workspace_root(input);
        find_ready_tasks(
            &loaded.rhei,
            &machines,
            // §AR-rhei-panta.5: a member's inputs resolve at its own root.
            &ReadySetRoots { workspace_root: &workspace_root, task_roots: &loaded.task_roots },
            // Nothing is in flight beyond what the plan itself records: a
            // listing has no run behind it. §FS-rhei-supervision.3.2
            &HashSet::new(),
        )
        .into_iter()
        .map(|task| &task.id)
        .collect()
    } else {
        HashSet::new()
    };

    // Judged against every machine in the project rather than the `--rhei`
    // scope: a real state no in-scope rhei uses is an honest empty result.
    // §FS-rhei-list.2.1: a state no machine declares is an error, not silence.
    for requested in &filters.states {
        let requested = requested.trim();
        let known = machines
            .distinct()
            .into_iter()
            .any(|machine| machine.is_valid_state(normalized_state_name(requested, machine)));
        if !known {
            let mut available: BTreeSet<&str> = BTreeSet::new();
            for machine in machines.distinct() {
                available.extend(machine.allowed_states());
            }
            let known =
                available.iter().map(|state| state.to_string()).collect::<Vec<_>>();
            return Err(miette!(
                help = did_you_mean(requested, &known)
                    .unwrap_or_else(|| "this machine declares no states.".to_string()),
                "unknown state '{}'; states in this {}: {}",
                requested,
                if loaded.is_panta_project() { "project" } else { "plan" },
                available.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
    }

    // Normalize state filter values once per machine so users can pass either
    // canonical names or counted-visit forms; a filter value normalizes under
    // each distinct machine and matches per ticket. §DA-per-rhei-state-machines
    let state_filter: Vec<String> = filters
        .states
        .iter()
        .flat_map(|s| {
            machines
                .distinct()
                .into_iter()
                .map(|machine| normalized_state_name(s.as_str(), machine))
                .collect::<Vec<_>>()
        })
        .collect();
    // §FS-rhei-panta.6: ticket targets accept the qualified id or an
    // unambiguous rhei-local shorthand — including these filter values.
    let parent_filter = filters
        .parent
        .as_deref()
        .map(|id| resolve_cli_task_id(&loaded, id, &rhei_scope))
        .transpose()?
        .map(|id| parse_task_id(&id));
    let has_prior_filter = filters
        .has_prior
        .as_deref()
        .map(|id| resolve_cli_task_id(&loaded, id, &rhei_scope))
        .transpose()?
        .map(|id| parse_task_id(&id));
    let contains_lower = filters.contains.as_deref().map(|s| s.to_lowercase());

    let mut matches: Vec<&(&rhei_core::ast::Task, Option<TaskId>)> = Vec::new();
    for entry in &flat {
        let (task, parent_id) = entry;

        // §FS-rhei-panta.6.4: `--rhei` filters the listing to named rheis.
        if !task_in_rhei_scope(&rhei_scope, &task.id.to_string()) {
            continue;
        }

        if !state_filter.is_empty() {
            let task_state =
                normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
            if !state_filter.iter().any(|s| s == &task_state) {
                continue;
            }
        }

        if let Some(want) = filters.assignee.as_deref() {
            if task.assignee.as_deref() != Some(want) {
                continue;
            }
        }
        if filters.no_assignee && task.assignee.is_some() {
            continue;
        }

        if let Some(want) = filters.kind.as_deref() {
            if !task.kind.eq_ignore_ascii_case(want) {
                continue;
            }
        }

        if let Some(prior_id) = &has_prior_filter {
            if !task.prior.iter().any(|p| p == prior_id) {
                continue;
            }
        }

        if let Some(parent_id_filter) = &parent_filter {
            if parent_id.as_ref() != Some(parent_id_filter) {
                continue;
            }
        }
        if filters.root && parent_id.is_some() {
            continue;
        }

        if let Some(needle) = &contains_lower {
            let title_hit = task.title.to_lowercase().contains(needle);
            let body_hit = task.content.to_lowercase().contains(needle);
            if !title_hit && !body_hit {
                continue;
            }
        }

        let machine = machines.for_task(&task.id);
        let is_terminal = is_terminal_state(task.state.as_str(), machine);
        if filters.terminal && !is_terminal {
            continue;
        }
        if filters.non_terminal && is_terminal {
            continue;
        }

        if filters.ready && !ready_ids.contains(&task.id) {
            continue;
        }
        // The complement of `--ready`, so every non-terminal ticket is one or
        // the other — never both, never neither — and "why is this not moving?"
        // has an answer from a single flag. §FS-rhei-list.3.1
        if filters.blocked && (is_terminal || ready_ids.contains(&task.id)) {
            continue;
        }

        matches.push(entry);
    }

    if filters.limit > 0 && matches.len() > filters.limit {
        matches.truncate(filters.limit);
    }

    if as_json {
        let payload: Vec<serde_json::Value> = matches
            .iter()
            .map(|(task, parent_id)| {
                let mut entry = serde_json::json!({
                    "id": task.id.to_string(),
                    "kind": task.kind,
                    "title": task.title,
                    "state": task.state,
                    "assignee": task.assignee,
                    "prior": task.prior.iter().map(TaskId::to_string).collect::<Vec<_>>(),
                    "parent": parent_id.as_ref().map(TaskId::to_string),
                    // Depth within the owning rhei: the Panta qualification
                    // segment is routing, not plan structure. §FS-rhei-list.4.2
                    "depth": task.profile_level(),
                });
                // Present only where it means something, so a plan using no
                // person-waiting poll emits the object it always did.
                // §FS-rhei-list.4.2
                if let Some(label) = task_waiting_on_person(task, &machines) {
                    entry["waiting_on"] = serde_json::Value::String(label.to_string());
                }
                // The author's layer of the same map, under the id printed
                // above: the keys rhei writes are held back, and a map holding
                // only those emits no field at all. §FS-rhei-list.4.2
                if let Some(author) = frontmatter
                    .as_ref()
                    .and_then(|document| author_task_metadata(document, &task.id.to_string()))
                {
                    entry["metadata"] = author;
                }
                entry
            })
            .collect();
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|err| miette!(
                help = internal_error_help(),
                "failed to serialize task list: {err}"
            ))?;
        println!("{rendered}");
        return Ok(());
    }

    if matches.is_empty() {
        println!("{}", empty_listing_line(&loaded, &filters));
        if filters.none_active() {
            report_empty_rheis(&loaded);
        }
        return Ok(());
    }

    for (task, _) in &matches {
        // Indent by depth within the owning rhei, so top-level tickets stay
        // flush-left after Panta qualification. §FS-rhei-list.4.1
        let indent = "  ".repeat(usize::from(task.profile_level()).saturating_sub(1));
        let mut line = format!(
            "{}{} {}: {} [{}]",
            indent,
            title_case_kind(&task.kind),
            task.id,
            task.title,
            task.state
        );
        // Directly after the state, because it qualifies the state; priors and
        // the assignee are properties of the ticket. §FS-rhei-list.4.1
        if let Some(label) = task_waiting_on_person(task, &machines) {
            line.push_str(&format!(" (waiting on {label})"));
        }
        if !task.prior.is_empty() {
            let priors: Vec<String> = task.prior.iter().map(TaskId::to_string).collect();
            line.push_str(&format!(" (prior: {})", priors.join(", ")));
        }
        if let Some(assignee) = &task.assignee {
            line.push_str(&format!(" @{}", assignee));
        }
        println!("{line}");
    }
    if filters.none_active() {
        report_empty_rheis(&loaded);
    }

    Ok(())
}