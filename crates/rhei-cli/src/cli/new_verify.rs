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
