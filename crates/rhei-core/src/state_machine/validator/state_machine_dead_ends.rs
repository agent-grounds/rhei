// A state nothing can take a task out of is refused when the machine loads.
// The machine-wide rule and the per-profile one are the same question at two
// scopes, and the only thing either ever disagreed with the engine about is
// which edges count as a state's way out.

// §FS-rhei-states.1.3 §FS-rhei-states.8.2 §FS-rhei-transitions.4.6

/// Whether `state` is a declared `final: true` state.
fn state_is_terminal(machine: &StateMachine, state: &str) -> bool {
    machine.states.get(state).is_some_and(|def| def.terminal)
}

/// Whether `rule` is an edge that can take a task out of `state`.
///
/// An explicit `from: <state>` edge always counts, whatever its target. A
/// `from: "*"` edge counts only when its target is not `final: true`: `rhei run`
/// never advances a task along a wildcard into a terminal state, so such an edge
/// is an escape hatch rather than forward progress. Out of a `gating: true`
/// state every edge counts, because it is a human who moves the task with
/// `rhei transition`, and that command honours any declared edge.
/// §FS-rhei-transitions.4.6
fn transition_leaves_state(machine: &StateMachine, state: &str, rule: &TransitionRule) -> bool {
    if rule.from.0 == state {
        return true;
    }
    if rule.from.0 != "*" {
        return false;
    }
    machine.states.get(state).is_some_and(|def| def.gating) || !state_is_terminal(machine, &rule.to.0)
}

/// Every state reachable from `start` by edges that count as a way out, in
/// breadth-first order and excluding `start` itself. `allowed`, when present,
/// narrows the walk to one profile's state set.
fn states_reachable_from<'a>(
    machine: &'a StateMachine,
    start: &'a str,
    allowed: Option<&HashSet<&str>>,
) -> Vec<&'a str> {
    let mut seen: HashSet<&str> = HashSet::from([start]);
    let mut order: Vec<&str> = Vec::new();
    let mut queue: VecDeque<&str> = VecDeque::from([start]);

    while let Some(state) = queue.pop_front() {
        if state_is_terminal(machine, state) {
            continue;
        }
        for rule in &machine.transitions {
            if !transition_leaves_state(machine, state, rule) {
                continue;
            }
            let target = rule.to.0.as_str();
            if allowed.is_some_and(|set| !set.contains(target)) {
                continue;
            }
            if seen.insert(target) {
                order.push(target);
                queue.push_back(target);
            }
        }
    }

    order
}

/// Which final states count as arriving, for a walk that asks whether one can
/// be reached.
///
/// The specification distinguishes exactly two questions, so this carries
/// exactly two variants rather than an arbitrary predicate: a third way of
/// counting a final state would need a specification point of its own before
/// it could be written here.
// §FS-rhei-transitions.4.6 §FS-rhei-supervision.1.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalReached {
    /// Any `final: true` state: *can a task be taken out of this state*. The
    /// engine needs somewhere to put it, and abandonment is somewhere.
    /// §FS-rhei-states.1.3
    Any,
    /// Any `final: true` state but the reserved cancellation terminal: *can
    /// this supervisor finish*. Reaching `cancelled` abandons the supervised
    /// work rather than declaring it done. §FS-rhei-states.1.4
    NotCancellation,
}

impl FinalReached {
    /// Whether arriving at `state` answers this question.
    fn arrives_at(self, machine: &StateMachine, state: &str) -> bool {
        state_is_terminal(machine, state)
            && (self == FinalReached::Any || !is_cancelled_state_name(state))
    }
}

/// Whether a final state `counts` accepts is reachable from `start`, counting
/// only the edges that can take a task out of the state they leave, and only
/// the states `allowed` permits when a profile narrows the machine.
/// §FS-rhei-transitions.4.6
fn state_can_reach_final<'a>(
    machine: &'a StateMachine,
    start: &'a str,
    allowed: Option<&HashSet<&str>>,
    counts: FinalReached,
) -> bool {
    counts.arrives_at(machine, start)
        || states_reachable_from(machine, start, allowed)
            .into_iter()
            .any(|state| counts.arrives_at(machine, state))
}

/// Name states in an error, bounded by the listing rule: past eight, name the
/// first few and say how many remain. §FS-rhei-errors.1.3
fn name_states(states: &[&str]) -> String {
    // Past this many the listing stops being a listing, so it names the first
    // few and defers to the command that prints them all.
    const LIMIT: usize = 8;
    const SHOWN: usize = 5;
    let quoted = |names: &[&str]| {
        names.iter().map(|name| format!("'{name}'")).collect::<Vec<_>>().join(", ")
    };
    if states.len() <= LIMIT {
        quoted(states)
    } else {
        format!("{}, and {} more", quoted(&states[..SHOWN]), states.len() - SHOWN)
    }
}

/// Why a task cannot be taken out of `state`, as one clause. `allowed`, when
/// present, narrows the question to one profile's state set.
fn dead_end_reason(machine: &StateMachine, state: &str, allowed: Option<&HashSet<&str>>) -> String {
    let mut out_of_it: Vec<&str> = Vec::new();
    for rule in &machine.transitions {
        let target = rule.to.0.as_str();
        if transition_leaves_state(machine, state, rule) && !out_of_it.contains(&target) {
            out_of_it.push(target);
        }
    }

    if out_of_it.iter().any(|target| allowed.is_none_or(|set| set.contains(target))) {
        // Every edge out of it exists and leads nowhere final: a chain or a
        // loop that is closed, rather than a state with no edge at all.
        let onward = states_reachable_from(machine, state, allowed);
        if onward.is_empty() {
            // Every counted edge re-reaches the state the walk started at, and
            // `states_reachable_from` leaves that one out: there is nothing to
            // list, and the loop itself is the whole reason.
            return "every path out of it leads back to itself".to_string();
        }
        return format!(
            "every path out of it leads only to {}, and none of those is final",
            name_states(&onward)
        );
    }
    if !out_of_it.is_empty() {
        return format!(
            "the only edges out of it lead to {}, which this profile does not allow",
            name_states(&out_of_it)
        );
    }

    let mut escapes: Vec<&str> = Vec::new();
    for rule in &machine.transitions {
        let target = rule.to.0.as_str();
        if rule.from.0 == "*" && state_is_terminal(machine, target) && !escapes.contains(&target) {
            escapes.push(target);
        }
    }
    if escapes.is_empty() {
        format!("the machine declares no `from: {state}` transition, and no wildcard matches it")
    } else {
        format!(
            "the machine declares no `from: {state}` transition, and its wildcard transition to \
             the final state {} is an escape hatch rather than forward progress, which `rhei run` \
             never takes",
            name_states(&escapes)
        )
    }
}

/// `text` with its first character upper-cased, for a clause that opens a
/// sentence rather than continuing one.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The edge that gives a stranded state a way on, as the user would write it.
fn dead_end_repair(state: &str) -> String {
    format!("give it a way on with `- {{from: {state}, to: <next-state>}}`")
}

/// Where to go when the error's own listing is not enough. §FS-rhei-errors.1.2
const DEAD_END_HELP: &str = "List every state and edge with: rhei states";

/// The refusal a machine with stranded states earns: every one of them named
/// at once, each with why it cannot be left and the line that gives it a way
/// on. §FS-rhei-states.1.3 §FS-rhei-errors.1
fn dead_end_message(machine: &StateMachine, stranded: &[&str]) -> String {
    const DETAILED: usize = 8;
    let plural = if stranded.len() == 1 { "state" } else { "states" };
    let mut message = format!(
        "state machine '{}' declares {} {plural} a task cannot be taken out of, so every task \
         that reaches one is stranded there:",
        machine.name,
        stranded.len()
    );
    for state in stranded.iter().take(DETAILED) {
        message.push_str(&format!(
            "\n  '{state}': {} — {}.",
            dead_end_reason(machine, state, None),
            dead_end_repair(state)
        ));
    }
    if stranded.len() > DETAILED {
        message.push_str(&format!("\n  ... and {} more", stranded.len() - DETAILED));
    }
    message.push('\n');
    message.push_str(DEAD_END_HELP);
    message
}

/// The state on a path out of `state` whose own way out this profile narrows
/// away: where the walk dies, and so where a repair belongs. It is not always
/// `state` itself — a profile that excludes a final state two hops on leaves
/// the entry state's edges perfectly good. `None` when no state on the path
/// has an edge the profile excludes, which means the machine gives them none
/// and widening `allowed` would repair nothing. §FS-rhei-states.8.2
fn narrowed_away_at<'a>(
    machine: &'a StateMachine,
    state: &'a str,
    allowed: &HashSet<&str>,
) -> Option<&'a str> {
    std::iter::once(state)
        .chain(states_reachable_from(machine, state, Some(allowed)))
        .find(|on_path| {
            machine.transitions.iter().any(|rule| {
                transition_leaves_state(machine, on_path, rule)
                    && !allowed.contains(rule.to.0.as_str())
            })
        })
}

/// The same refusal at profile scope: the profile and the state it cannot
/// leave, then why and the line to add. §FS-rhei-states.8.2
fn profile_dead_end_message(
    machine: &StateMachine,
    profile_name: &str,
    state: &str,
    allowed: &HashSet<&str>,
) -> String {
    // Widening `allowed` only repairs a state the profile narrowed away from
    // its way out; where the machine itself gives it none, saying so would
    // send the reader to the wrong file. Where the profile did narrow one
    // away, the file to edit is still the profile's, but the state to name is
    // the one the walk died at rather than the one it started from.
    let (widen, repair) = match narrowed_away_at(machine, state, allowed) {
        Some(at) if at == state => (
            "Widen this profile's `allowed` to a state that reaches a final one, or ".to_string(),
            dead_end_repair(state),
        ),
        Some(at) => (
            format!(
                "The path dies at '{at}', whose own edges leave `allowed`: widen this profile's \
                 `allowed` to keep one of them, or "
            ),
            dead_end_repair(at),
        ),
        None => (String::new(), capitalized(&dead_end_repair(state))),
    };
    format!(
        "profile '{profile_name}' allows non-final state '{state}', but no path using only \
         allowed states reaches a final state: {}. {widen}{repair}. {DEAD_END_HELP}",
        dead_end_reason(machine, state, Some(allowed))
    )
}

impl StateMachine {
    /// Reject a machine that declares a state a task cannot be taken out of.
    ///
    /// The engine has no edge to advance such a task and `rhei transition` has
    /// none to offer a human either, so the task — and everything waiting on it
    /// — stops there. Refusing the machine when it loads costs one YAML line;
    /// accepting it costs whatever the run spent before it stranded.
    /// §FS-rhei-states.1.3
    fn validate_every_state_can_be_left(&self) -> Result<(), StateMachineLoadError> {
        let stranded: Vec<&str> = self
            .states
            .iter()
            .filter(|(name, def)| {
                !def.terminal && !state_can_reach_final(self, name, None, FinalReached::Any)
            })
            .map(|(name, _)| name.as_str())
            .collect();

        if stranded.is_empty() {
            return Ok(());
        }
        Err(StateMachineLoadError::Invalid(dead_end_message(self, &stranded)))
    }
}
