// `**Prior:**` validation: whether every dependency resolves, whether the graph
// they form is acyclic in spirit, whether the order they were completed in
// makes sense — and the nearest-id suggestions an unresolved one earns.
//
// Its own part because a dependency is judged against the whole task index and
// the project's rhei ids, while every other check next door judges one node
// against its machine.

// §AR-source-file-size.3 §FS-rhei-plan-language.3

fn validate_dependency_integrity(
    rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    report: &mut ValidationReport,
) {
    let rhei_ids = project_rhei_ids(rhei);

    fn recurse(
        task: &Task,
        ancestors: &mut Vec<TaskId>,
        index: &HashMap<TaskId, &Task>,
        rhei_ids: &[String],
        structure: &Structure,
        report: &mut ValidationReport,
    ) {
        let mut seen: HashSet<&TaskId> = HashSet::new();
        for (position, dep) in task.prior.iter().enumerate() {
            let kind = task.prior_kinds.get(position).and_then(|k| k.as_deref());
            // A repeated reference is at best noise and at worst a leftover
            // from an edit that meant to name a different task.
            // §FS-rhei-plan-language.3.1
            if !seen.insert(dep) {
                report.errors.push(format!(
                    "Task {} lists Task {} more than once in **Prior:**; drop the duplicate",
                    task.id, dep
                ));
            }
            match (index.get(dep), kind) {
                // The kind keyword is decoration for the reader, so a wrong
                // one misleads exactly where it was meant to help.
                // §FS-rhei-plan-language.3.1
                (Some(target), Some(kind)) if !target.kind.eq_ignore_ascii_case(kind) => {
                    let flavor = if structure.accepts_kind(kind) {
                        String::new()
                    } else {
                        // The declared set is merged from every rhei, so it
                        // changes when an unrelated one is added: it is
                        // guidance, not part of this error. §FS-rhei-new.5.2
                        report.help.push(declared_node_kinds_help(structure));
                        format!(" ('{kind}' is not a declared node kind)")
                    };
                    report.errors.push(format!(
                        "Task {} **Prior:** kind keyword '{kind}' does not match Task {}: \
                         that node is declared '{}'{flavor}. Use the node's kind or the bare id",
                        task.id,
                        dep,
                        title_case_kind(&target.kind),
                    ));
                }
                // An unknown kind on an unresolvable reference is the shape a
                // pasted task *title* takes — lead with that common mistake.
                // §FS-rhei-plan-language.3.1
                (None, Some(kind)) if !structure.accepts_kind(kind) => {
                    report.help.push(declared_node_kinds_help(structure));
                    report.errors.push(format!(
                        "Task {} has an unresolvable **Prior:** reference: '{kind}' is not a \
                         declared node kind and no Task {} exists. If the reference is a task \
                         title, use the task's id instead (`**Prior:** 1`, `**Prior:** auth.2`)",
                        task.id, dep
                    ));
                }
                (None, _) => {
                    let (tail, guidance) = missing_prior_hint(&task.id, dep, index, rhei_ids);
                    if let Some(guidance) = guidance {
                        report.help.push(guidance);
                    }
                    report
                        .errors
                        .push(format!("Task {} depends on missing Task {}{tail}", task.id, dep));
                }
                _ => {}
            }
            if ancestors.iter().any(|ancestor| ancestor == dep) {
                report.errors.push(format!(
                    "Task {} cannot list ancestor Task {} as **Prior:**; parent/child structure already defines containment. Make the dependent work a top-level sibling if it must wait for Task {}.",
                    task.id, dep, dep
                ));
            }
        }

        // A task export is a declared graph handoff, not a best-effort file
        // lookup. Check its producer, declaration, and directly authored
        // ordering edge while the ancestors are still available. Missing and
        // forbidden producers are primary errors, so their derivative checks
        // are deliberately suppressed. §FS-rhei-plan-language.3.12.1
        for consumed in &task.consumes {
            let producer = &consumed.task;
            if producer == &task.id {
                report.errors.push(format!(
                    "Task {} cannot consume its own export '{}'",
                    task.id, consumed.name
                ));
                continue;
            }
            if ancestors.iter().any(|ancestor| ancestor == producer) {
                report.errors.push(format!(
                    "Task {} cannot consume export '{}' from ancestor Task {}",
                    task.id, consumed.name, producer
                ));
                continue;
            }
            let Some(producing_task) = index.get(producer) else {
                // The ordinary Prior diagnostic already names this producer.
                // One authored bad reference gets one primary error.
                // §FS-rhei-validate.4.3
                if !task.prior.iter().any(|prior| prior == producer) {
                    let (tail, guidance) =
                        missing_prior_hint(&task.id, producer, index, rhei_ids);
                    if let Some(guidance) = guidance {
                        report.help.push(guidance.replace("**Prior:**", "**Consumes:**"));
                    }
                    report.errors.push(format!(
                        "Task {} consumes export '{}' from missing producer Task {}{}",
                        task.id, consumed.name, producer, tail
                    ));
                }
                continue;
            };

            if !producing_task.provides.iter().any(|name| name == &consumed.name) {
                let mut available = producing_task.provides.clone();
                available.sort();
                let available = if available.is_empty() {
                    "(none)".to_string()
                } else {
                    available.join(", ")
                };
                report.errors.push(format!(
                    "Task {} consumes export '{}' from Task {}, but that task does not declare it in **Provides:**. Available exports: {}",
                    task.id,
                    consumed.name,
                    producer,
                    available
                ));
            }
            if !task.prior.iter().any(|prior| prior == producer) {
                report.errors.push(format!(
                    "Task {} consumes export '{}' from Task {} and must list Task {} directly in **Prior:**",
                    task.id, consumed.name, producer, producer
                ));
            }
        }
        ancestors.push(task.id.clone());
        for child in &task.children {
            recurse(child, ancestors, index, rhei_ids, structure, report);
        }
        ancestors.pop();
    }

    let mut ancestors = Vec::new();
    for task in &rhei.tasks {
        recurse(task, &mut ancestors, index, &rhei_ids, &rhei.structure, report);
    }
}

/// Rhei ids of the merged project: the leading segment of every top-level id.
fn project_rhei_ids(rhei: &Rhei) -> Vec<String> {
    let mut ids: Vec<String> = rhei
        .tasks
        .iter()
        .filter_map(|task| match task.id.segments.first() {
            Some(TaskIdSegment::Named(name)) => Some(name.clone()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Explain a missing prior that resolved to no rhei at all.
///
/// A dotted `**Prior:**` whose leading segment names no rhei is kept as the
/// author wrote it, so the id in the error is quotable back to the source. It is
/// still ambiguous — a typo'd rhei name or a typo'd local hierarchical id — so
/// the hint rules out both readings and only offers a correction that resolves.
// §FS-rhei-validate.4.1: an unresolved prior is reported as the author wrote it.
fn missing_prior_hint(
    task: &TaskId,
    dep: &TaskId,
    index: &HashMap<TaskId, &Task>,
    rhei_ids: &[String],
) -> (String, Option<String>) {
    let Some(TaskIdSegment::Named(candidate)) = dep.segments.first() else {
        return (String::new(), None);
    };
    // A dep under a known rhei is a plain missing ticket and needs no explaining.
    if rhei_ids.iter().any(|id| id == candidate) {
        return (String::new(), None);
    }
    let citing_rhei = match task.segments.first() {
        Some(TaskIdSegment::Named(name)) => name.as_str(),
        _ => return (String::new(), None),
    };
    let tail = format!(
        ": no rhei named '{candidate}' in this project, \
         and rhei '{citing_rhei}' has no ticket '{dep}'"
    );
    // The rhei list — and the nearest id in it — is what a create elsewhere
    // changes, so both halves stay guidance rather than error text.
    // §FS-rhei-new.5.2
    let mut guidance =
        format!("Task {task} **Prior:** '{dep}': this project's rheis are {}.", rhei_ids.join(", "));
    if let Some(corrected) = nearest_resolving_id(task, dep, candidate, index, rhei_ids) {
        guidance.push_str(&format!(" Did you mean '{corrected}'?"));
    }
    (tail, Some(guidance))
}

/// The node kinds the merged project declares, as guidance rather than as part
/// of an error: `rhei new --node-kinds` on any rhei changes the list.
// §FS-rhei-plan-language.3.7 §FS-rhei-new.5.2
fn declared_node_kinds_help(structure: &Structure) -> String {
    format!("this plan structure declares nodeKinds {:?}.", structure.node_kinds)
}

/// Correct `dep`'s leading segment to the nearest rhei id, keeping the
/// suggestion only when it names a real ticket other than the citing one.
///
/// Suggesting an id that does not resolve trades one dead end for another, and
/// suggesting the citing task itself proposes a self-dependency.
fn nearest_resolving_id(
    task: &TaskId,
    dep: &TaskId,
    candidate: &str,
    index: &HashMap<TaskId, &Task>,
    rhei_ids: &[String],
) -> Option<TaskId> {
    let nearest = nearest_rhei_id(candidate, rhei_ids)?;
    let mut segments = dep.segments.clone();
    segments[0] = TaskIdSegment::Named(nearest.to_string());
    let corrected = TaskId::from_segments(segments);
    (corrected != *task && index.contains_key(&corrected)).then_some(corrected)
}

/// Closest rhei id to `candidate` within a small edit distance, if any.
fn nearest_rhei_id<'a>(candidate: &str, rhei_ids: &'a [String]) -> Option<&'a str> {
    // Below three characters every id is within one edit of every other, so a
    // "near miss" carries no signal; above it, two edits catches transpositions
    // and a single slip without pairing unrelated names.
    let length = candidate.chars().count();
    if length < 3 {
        return None;
    }
    let budget = 2.min(length.div_ceil(3)).max(1);
    rhei_ids
        .iter()
        .map(|id| (edit_distance(candidate, id), id.as_str()))
        .filter(|(distance, _)| *distance <= budget)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, id)| id)
}

/// Levenshtein distance over chars.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, a_char) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, b_char) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(a_char != b_char);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// Warn about tickets that went terminal while a `**Prior:**` is unsatisfied.
/// A terminal ticket leaves readiness and `--blocked`, so nothing else reveals
/// it; legitimate authoring reaches it, so warn. §FS-rhei-validate.4
fn validate_prior_order_coherence(
    rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    machines: &MachineSet,
    report: &mut ValidationReport,
) {
    // A prior is judged under the machine of the rhei that owns it: the
    // target's states mean what its own process says. §FS-rhei-panta.6.1
    let satisfied = |id: &TaskId| -> bool {
        index
            .get(id)
            .map(|dep| {
                let machine = machines.for_task(id);
                let state = parse_task_state(dep.state.as_str(), machine).state;
                // §FS-rhei-states.1.4: the reserved cancel name, either spelling.
                !is_cancelled_state_name(&state)
                    && machine.states.get(&state).map(|def| def.terminal).unwrap_or(false)
            })
            .unwrap_or(false)
    };

    for_each_node(rhei, |task| {
        let machine = machines.for_task(&task.id);
        let state = parse_task_state(task.state.as_str(), machine).state;
        // Only a *successful* terminal state is a contradiction: a cancelled
        // ticket never claimed its prerequisites ran.
        if is_cancelled_state_name(&state)
            || !machine.states.get(&state).map(|def| def.terminal).unwrap_or(false)
        {
            return;
        }
        // A missing prior is already a hard error in dependency integrity;
        // do not double-report it here.
        let unmet: Vec<String> = task
            .prior
            .iter()
            .filter(|dep| index.contains_key(*dep) && !satisfied(dep))
            .map(|dep| {
                format!(
                    "Task {} ({})",
                    dep,
                    parse_task_state(index[dep].state.as_str(), machines.for_task(dep)).state
                )
            })
            .collect();
        if !unmet.is_empty() {
            report.warnings.push(format!(
                "{} {} is '{}' but its prerequisites are unsatisfied: {}. The plan contradicts its own **Prior:** dependencies.",
                title_case_kind(&task.kind),
                task.id,
                state,
                unmet.join(", ")
            ));
        }
    });
}
