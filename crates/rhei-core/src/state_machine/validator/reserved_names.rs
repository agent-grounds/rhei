// State names the engine reads as more than a label.
//
// Its own part because a reserved name is a contract between the machine an
// author writes and the five rules that key on it, and none of the files that
// apply those rules owns the contract.

// §AR-source-file-size.3 §FS-rhei-states.1.4

/// Whether a bare state name is the reserved cancellation state.
///
/// `cancelled` is reserved: a machine may name its abandon state whatever it
/// likes, but only this name — and `canceled`, accepted as the same name —
/// carries the engine's cancellation semantics. A cancelled prior does not
/// satisfy a dependency, `rhei complete` never selects it, the run report marks
/// it apart from success, a transition into it waives the abandoned step's
/// declared outputs, and a supervisor whose `openDescendants` exit reaches only
/// this state is warned about as one with no exit at all
/// (§FS-rhei-supervision.1.2). One predicate so those five never disagree.
///
/// The argument is a *normalized* state name: strip any `-<n>` visit suffix
/// first (`normalized_state_name` in the CLI does that).
// §FS-rhei-states.1.4
pub fn is_cancelled_state_name(state: &str) -> bool {
    matches!(state, "cancelled" | "canceled")
}
