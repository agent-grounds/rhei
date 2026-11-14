// The supervisor a `--task` reset leaves out but holds, so its next visit
// briefs the tickets the reset reopened under it again.
//
// Its own part because it is the one write a reset makes to a ticket outside
// its scope: a metadata edit on the supervisor, with its state line, counted
// suffix, result and history left as they stand.

// §AR-source-file-size.3 §FS-rhei-reset.2.5.3

/// A supervisor a `--task` reset holds: where its metadata lives, and one reset
/// checkpoint for each ticket it owns that the reset moves or removes records of.
/// §FS-rhei-reset.2.5.3
struct HeldSupervisor {
    /// Project-qualified, as the preview names it.
    task_id: String,
    metadata_file: PathBuf,
    /// Its `metadata.tasks.<id>` key inside `metadata_file`.
    metadata_id: TaskId,
    checkpoints: Vec<SupervisionCheckpoint>,
}

/// The supervisors a ticket scope leaves out that own a ticket it moves or removes
/// records of, in id order. A ticket in scope that stands where it was authored
/// still loses its brief, so its checkpoint runs from its state to the same one;
/// one that loses nothing either is no checkpoint, and a supervisor owning only
/// such tickets is not held. A whole-rhei reset holds none: it reaches every
/// supervisor its tickets have. §FS-rhei-reset.2.5.3 §FS-rhei-supervision.3.3
fn held_supervisors(
    loaded: &LoadedPlan,
    input: &Path,
    scope: ResetScope<'_>,
    authored: &AuthoredStates,
) -> Vec<HeldSupervisor> {
    let Some(tickets) = scope.tickets() else { return Vec::new() };
    let moves: HashMap<&str, &StateMove> =
        authored.moves.iter().map(|mv| (mv.task_id.as_str(), mv)).collect();
    let mut held = Vec::new();
    for (supervisor, owned) in &tickets.supervisors {
        let checkpoints: Vec<SupervisionCheckpoint> = owned
            .iter()
            .filter_map(|id| {
                let (from, to) = match moves.get(id.as_str()) {
                    Some(mv) => (mv.from.clone(), mv.to.clone()),
                    None => {
                        let state = authored.emptied_in_place.get(id)?;
                        (state.clone(), state.clone())
                    }
                };
                Some(SupervisionCheckpoint {
                    task: loaded.task_route(id, input).local_id,
                    from,
                    to,
                    // The reset cleared its counts: `to` was entered by no counted visit.
                    visit: 0,
                    reset: true,
                })
            })
            .collect();
        if checkpoints.is_empty() {
            continue;
        }
        let route = loaded.task_route(supervisor, input);
        held.push(HeldSupervisor {
            task_id: supervisor.clone(),
            metadata_file: route.metadata_file,
            metadata_id: parse_task_id(&route.metadata_id),
            checkpoints,
        });
    }
    held
}

/// The metadata documents a ticket scope may hold a supervisor in, whether or not
/// one of its tickets moves, so the writer locks cover them before the decision
/// that says which are held is read. §FS-rhei-reset.2.5.5
fn held_supervisor_metadata_files(
    loaded: &LoadedPlan,
    input: &Path,
    scope: ResetScope<'_>,
) -> Vec<PathBuf> {
    let Some(tickets) = scope.tickets() else { return Vec::new() };
    tickets
        .supervisors
        .keys()
        .map(|supervisor| loaded.task_route(supervisor, input).metadata_file)
        .collect()
}

/// Print the supervisors a reset holds, under `verb` ("Would hold" for the
/// preview, "Held" for the summary). §FS-rhei-reset.4
fn report_held_supervisors(held: &[HeldSupervisor], verb: &str) {
    for supervisor in held {
        println!(
            "{verb} Task {}, which supervises them, so its next visit briefs them again.",
            supervisor.task_id
        );
    }
}
