// Deciding whether a create actually succeeded: what the project was already
// failing at, what this write added to that, and whether the new id reads back
// out of the plan.
//
// Its own part because it is the one place `rhei new` compares two states of
// the world. Deciding and writing the markdown know nothing about either.

// §FS-rhei-new.5.1 §FS-rhei-new.5.2

/// A create that has to be undone: the diagnostic to print, and the clause
/// completing "the create was rolled back because …".
struct CreateFailure {
    report: Report,
    reason: &'static str,
}

/// Every error the project's validation pass finds right now, as plain strings.
///
/// A project that does not load at all reduces to the one report it failed
/// with, so the same set difference decides that case too: a create is not
/// answerable for a parse error it did not introduce.
// §FS-rhei-new.5.2
fn create_validation_errors(target: &Path) -> Vec<String> {
    match validation_pass(target, None) {
        Ok(pass) => pass.errors,
        Err(report) => vec![report.to_string()],
    }
}

/// Judge the create that has just been written: first the errors it
/// *introduced* over `inherited`, then whether the id it claims to have created
/// reads back out of the plan.
///
/// `None` is success — including the case where the project was already failing
/// validation and still fails in exactly the same way, which is not this
/// create's business to refuse.
// §FS-rhei-new.5.1 §FS-rhei-new.5.2
fn new_write_failure(
    target: &Path,
    write: &NewWrite,
    inherited: &[String],
    before: Option<&BTreeSet<String>>,
) -> Option<CreateFailure> {
    // First: it is the only check that does not need the project to load under
    // its state machines, and a write that deleted work is undone either way.
    // §FS-rhei-new.5.1
    if let Some(failure) = vanished_ids_failure(target, before) {
        return Some(failure);
    }
    // Before the pass, because the pass no longer refuses every tree this
    // fault produces, and because a create whose own flag is wrong should not
    // first be told about the project it momentarily broke. §FS-rhei-new.1.2
    if let Some(failure) = undeclared_created_machine_failure(target, write) {
        return Some(failure);
    }
    match validation_pass(target, None) {
        Ok(pass) => {
            let introduced = errors_introduced_over(inherited, pass.errors);
            if !introduced.is_empty() {
                return Some(CreateFailure {
                    report: validation_report(
                        target,
                        &pass.state_machine_sources,
                        &introduced,
                        &pass.help,
                    ),
                    reason: "the project would not validate with it",
                });
            }
        }
        // Unloadable before the create and unloadable in the same way after it:
        // there is nothing here the write is answerable for, and nothing to
        // reload the new id out of either.
        Err(report) if inherited.contains(&report.to_string()) => return None,
        Err(report) => {
            return Some(CreateFailure { report, reason: "the project would not load with it" });
        }
    }
    verify_created_id(target, write).err()
}

/// Refuse a create whose `--states` names a machine no `states.yaml` in the
/// project declares.
///
/// The rule is §FS-rhei-new.1.2's and the fault is this create's: `--states`
/// writes the declaration and does not author the machine, so a name nothing
/// declares is a fault in the flag. It needs saying here because the project's
/// validation pass no longer says it for every such tree — while the
/// `**States:**` declaration is deprecated, a declaration nothing supplies
/// falls through to whatever `states.yaml` sits in the rhei's own root
/// (§FS-rhei-states-deprecation.2.1). That indulgence is for a tree already on
/// disk, which a release has been spent warning. Creation is authoring: the
/// one command whose job is to write a correct index must not write a
/// `**States:**` line and, in the same breath, warn that the line should be
/// deleted.
///
/// A check on the *name*, not a second precedence rule: which file the created
/// rhei runs under is still the one resolution path §FS-rhei-new.2.1.1 names.
///
/// Exactly one candidate is read strictly: the `states.yaml` in the prospective
/// root this create is adopting. Its parse error is genuinely this create's
/// business, so it is raised as the parse error it is — the same
/// `state_machine_load_report` the validation pass behind this check would
/// have shown — rather than counted as declaring nothing and reported as a
/// missing machine. Raising it here rather than leaving it to the pass is what
/// keeps a project already failing in the same way from swallowing it.
///
/// Every other source the lookup touches — the project root's file, which
/// predates the create, another rhei's, or the project plan itself — is skipped
/// and *remembered* when it will not read. A file broken elsewhere in the
/// project is not a licence to keep a `**States:**` line nothing declares, and
/// abandoning the whole check on one unreadable candidate is how the command
/// comes to write a line the next one tells the author to delete.
// §FS-rhei-new.1.2 §FS-rhei-new.2.1.1 §FS-rhei-new.5.2
fn undeclared_created_machine_failure(
    target: &Path,
    write: &NewWrite,
) -> Option<CreateFailure> {
    let declared = write.declared_machine.as_deref()?.trim();
    if declared.is_empty() {
        return None;
    }
    let project_candidate = auto_state_machine_path(target);
    let own_candidate = write.path.parent().unwrap_or_else(|| Path::new(".")).join("states.yaml");
    // The one strict read, and only where the create is adopting a root of its
    // own: a project-root file equal to it was there before this invocation.
    if own_candidate != project_candidate && own_candidate.is_file() {
        if let Err(report) = load_state_machine(Some(&own_candidate)) {
            return Some(CreateFailure {
                report,
                reason: "the state machine in its own root could not be read",
            });
        }
    }
    let mut unread: Vec<String> = Vec::new();
    let mut candidates = vec![own_candidate, project_candidate];
    match load_plan_leniently(target) {
        Ok(loaded) => candidates
            .extend(sorted_rhei_roots(&loaded).into_iter().map(|root| root.join("states.yaml"))),
        // The plan is a candidate source like any other, and a project that
        // will not load is one whose other rhei roots could not be listed —
        // remembered, not read as "nothing else declares it".
        Err(_) => unread.push(format!("the project at '{}', which does not load", target.display())),
    }
    let mut names = vec![rhei_validator::StateMachine::builtin_default().name];
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) || !candidate.is_file() {
            continue;
        }
        match load_state_machine(Some(&candidate)) {
            Ok(machine) => names.push(machine.name),
            Err(_) => unread.push(format!("'{}'", candidate.display())),
        }
    }
    if names.iter().any(|name| name == declared) {
        return None;
    }
    names.sort();
    names.dedup();
    let unread = if unread.is_empty() {
        String::new()
    } else {
        format!(" Not read, so what they declare is unknown: {}.", unread.join(", "))
    };
    Some(CreateFailure {
        report: miette!(
            help = format!(
                "this project's states files declare: {}.{unread} Author the machine first — \
                 `/rhei-state-machine-writer` writes one — then re-run, or pass a `--states` \
                 naming one of those.",
                names.join(", ")
            ),
            "--states '{declared}' writes `**States:** {declared}`, but no states file \
             declaring it was found in the new rhei's root, the project root, or any other \
             rhei root. `--states` only writes the declaration; it does not create the \
             state machine, so the rhei would point at nothing."
        ),
        reason: "its `--states` names a machine no states file declares",
    })
}

/// Every id the project holds right now: its rheis, and every ticket in them.
///
/// Read leniently, because that is how the create read it: a rhei skipped
/// before the write is skipped after it too, so its ids are absent from both
/// sides and never look lost. `None` when the project does not load at all,
/// which leaves no baseline to compare against — and no baseline is the honest
/// answer, not an empty one.
// §FS-rhei-new.5.1
fn create_plan_ids(target: &Path) -> Option<BTreeSet<String>> {
    let loaded = load_plan_leniently(target).ok()?;
    let mut ids: BTreeSet<String> = loaded.rhei_ids.iter().cloned().collect();
    fn walk(tasks: &[rhei_core::ast::Task], ids: &mut BTreeSet<String>) {
        for task in tasks {
            ids.insert(task.id.to_string());
            walk(&task.children, ids);
        }
    }
    walk(&loaded.rhei.tasks, &mut ids);
    Some(ids)
}

/// Refuse a create that made an id the project already held stop existing.
///
/// The general guard behind §5.1's reload: checking that the *new* id reads
/// back says nothing about the ones that were already there, and the ways a
/// splice can delete them are open-ended — an unclosed ``` fence in a
/// description swallows every node after the insertion point, and the project
/// still parses and still validates. Comparing the whole id set does not need
/// to know which bug produced the loss, which is the point of having it.
// §FS-rhei-new.5.1
fn vanished_ids_failure(
    target: &Path,
    before: Option<&BTreeSet<String>>,
) -> Option<CreateFailure> {
    let before = before?;
    let after = create_plan_ids(target)?;
    let vanished: Vec<&str> =
        before.difference(&after).map(String::as_str).collect();
    if vanished.is_empty() {
        return None;
    }
    Some(CreateFailure {
        report: miette!(
help = "nothing in the plan was meant to change but the new block, so this is a splicing fault, not an authoring one. A description holding an unclosed ``` fence is the usual cause.",

            "the create removed {} the project already held: {}\n\n`rhei new` only ever adds, \
             so an id that stops reading back is work this write destroyed.",
            if vanished.len() == 1 { "an id" } else { "ids" },
            vanished.join(", ")
        ),
        reason: "it removed ids that were already in the project",
    })
}

/// The errors in `after` that `inherited` does not account for.
///
/// Each inherited error is spent once, so a message the project already carried
/// twice and now carries three times still reports one new occurrence — which
/// is the honest reading of "the errors this create introduced".
// §FS-rhei-new.5.2
fn errors_introduced_over(inherited: &[String], after: Vec<String>) -> Vec<String> {
    let mut unspent: Vec<&String> = inherited.iter().collect();
    after
        .into_iter()
        .filter(|error| match unspent.iter().position(|kept| *kept == error) {
            Some(index) => {
                unspent.remove(index);
                false
            }
            None => true,
        })
        .collect()
}

/// Confirm the create is in the plan the next command will read.
///
/// Validation passing is not the same as the node existing. A block appended
/// after an unterminated code fence, or spliced where the parser ends a section
/// earlier than the writer assumed, leaves the project valid and the ticket
/// absent — and reported as success, that is a file the author has to debug by
/// hand, with the next create about to hand out the same id again. Reloading is
/// the general guard: it does not need to know which splicing bug produced the
/// miss.
// §FS-rhei-new.5.1
fn verify_created_id(target: &Path, write: &NewWrite) -> Result<(), CreateFailure> {
    if load_plan(target).is_ok_and(|loaded| created_id_reads_back(&loaded, write, target)) {
        return Ok(());
    }
    Err(CreateFailure {
        report: miette!(
help = "check the block did not land inside a code fence, or past the end of the section it was aimed at.",

            "{} '{}' was written to {}, but reloading the project does not find it there",
            write.kind,
            write.id,
            display_path(&write.path)
        ),
        reason: "the plan does not read it back",
    })
}

/// True when the reloaded plan holds the created id — and, for a ticket, holds
/// it in the very file that was written. §FS-rhei-new.5.1
fn created_id_reads_back(loaded: &LoadedPlan, write: &NewWrite, target: &Path) -> bool {
    if write.kind == "rhei" {
        return loaded.rhei_ids.iter().any(|id| id == &write.id);
    }
    find_task_by_id_str(&loaded.rhei.tasks, &write.id).is_some()
        && same_path(&loaded.task_file(&write.id, target), &write.path)
}

/// Compare two paths for the same file, resolving `.`, `..`, and symlinks where
/// the filesystem can; a path that will not canonicalize is compared as spelled.
fn same_path(left: &Path, right: &Path) -> bool {
    match (rhei_core::platform::canonical_path(left), rhei_core::platform::canonical_path(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
