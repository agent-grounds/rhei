// Selecting the one supervisor that owns a descendant's move. Kept separate
// from checkpoint delivery so the locked transition path can validate an
// explicit operation context before it evaluates or applies the move.

// §AR-source-file-size.3 §FS-rhei-supervision.2.2

/// The chain from `target`'s parent up to its root, nearest ancestor first.
fn ancestor_chain<'a>(
    tasks: &'a [rhei_core::ast::Task],
    target: &TaskId,
) -> Vec<&'a rhei_core::ast::Task> {
    fn walk<'a>(
        tasks: &'a [rhei_core::ast::Task],
        target: &TaskId,
        stack: &mut Vec<&'a rhei_core::ast::Task>,
    ) -> bool {
        for task in tasks {
            if &task.id == target {
                return true;
            }
            stack.push(task);
            if walk(&task.children, target, stack) {
                return true;
            }
            stack.pop();
        }
        false
    }
    let mut stack = Vec::new();
    if walk(tasks, target, &mut stack) {
        stack.reverse();
        return stack;
    }
    Vec::new()
}

/// The nearest supervising ancestor whose scope includes the moving task.
///
/// Scope alone selects the owner. In particular, a nonmatching event filter
/// does not make the search climb to a farther ancestor.
// §FS-rhei-supervision.2.2
fn nearest_in_scope_supervising_owner<'a>(
    machine: &rhei_validator::StateMachine,
    ancestors: &'a [rhei_core::ast::Task],
) -> Option<&'a rhei_core::ast::Task> {
    ancestors.iter().enumerate().find_map(|(distance, ancestor)| {
        let execute_on = execute_on_of(
            machine,
            &normalized_state_name(ancestor.state.as_str(), machine),
        )?;
        let in_scope = match execute_on.scope() {
            // Distance 0 is the transitioning task's own parent.
            rhei_validator::SupervisionScope::Child => distance == 0,
            rhei_validator::SupervisionScope::Descendant => true,
        };
        in_scope.then_some(ancestor)
    })
}

/// Render an in-file ancestor id in the moving task's qualified id space.
// §FS-rhei-panta.6 §FS-rhei-transition-cmd.2
fn qualified_supervising_owner_id(
    owner: &rhei_core::ast::Task,
    moving_local_id: &str,
    moving_qualified_id: &str,
) -> TaskId {
    let prefix = moving_qualified_id.strip_suffix(moving_local_id).unwrap_or("");
    parse_task_id(&format!("{prefix}{}", owner.id))
}

/// Refuse an explicit operation context that does not name the selected owner.
// §FS-rhei-transition-cmd.2
fn ensure_operation_supervisor_matches(
    supplied: Option<&TaskId>,
    owner: Option<&rhei_core::ast::Task>,
    moving_local_id: &str,
    moving_qualified_id: &str,
) -> MietteResult<()> {
    let Some(supplied) = supplied else { return Ok(()) };
    let Some(owner) = owner else {
        return Err(miette!(
            help = "omit --supervisor; this task has no supervisor checkpoint to suppress",
            "Task {moving_qualified_id} cannot use --supervisor {supplied}; it has no in-scope supervising ancestor"
        ));
    };
    let expected = qualified_supervising_owner_id(owner, moving_local_id, moving_qualified_id);
    if supplied == &expected {
        return Ok(());
    }
    Err(miette!(
        help = format!(
            "pass --supervisor {expected} or omit the flag when the move is not that supervisor's own"
        ),
        "Task {moving_qualified_id} cannot use --supervisor {supplied}; its nearest in-scope \
         supervising ancestor is Task {expected}"
    ))
}
