// Which edge a worker's exit selects, and how it matched. A question about the
// machine and the plan's counts rather than about a process, so it sits apart
// from the spawn and wait next door, and an agent's exit asks it as well.

// §AR-source-file-size.3 §FS-rhei-programs.3.2 §FS-rhei-run.5.1

// Mirrors rhei-core's ExitCodeSet::of (validator/exit_code_overlap.rs); change both together.
fn transition_matches_exit_code(rule: &rhei_core::ast::TransitionRule, exit_code: i32) -> bool {
    match rule.exit_code.as_ref() {
        Some(YamlValue::Number(number)) => number.as_i64() == Some(i64::from(exit_code)),
        Some(YamlValue::Sequence(values)) => {
            values.iter().filter_map(YamlValue::as_i64).any(|value| value == i64::from(exit_code))
        }
        Some(YamlValue::String(value)) if value == "nonzero" => exit_code != 0,
        _ => false,
    }
}

fn transition_has_exact_exit_code(rule: &rhei_core::ast::TransitionRule) -> bool {
    matches!(rule.exit_code, Some(YamlValue::Number(_)) | Some(YamlValue::Sequence(_)))
}

fn transition_is_nonzero_exit_code(rule: &rhei_core::ast::TransitionRule) -> bool {
    matches!(rule.exit_code, Some(YamlValue::String(ref value)) if value == "nonzero")
}

fn program_transition_is_applicable(
    rule: &rhei_core::ast::TransitionRule,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
) -> bool {
    transition_rule_is_applicable(
        rule,
        machine,
        metadata,
        &task.id,
        Some(task),
        current_state,
        task.state.as_str(),
    )
    .unwrap_or(false)
}

// Keyed strictly to `poll:` states, mutually exclusive with `visits`, so an
// exhausted supervising `visits` budget never widens into this carve-out
// (agent-grounds/rhei#122). §FS-rhei-run.5.1
fn spent_poll_budget(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
) -> Option<SpentLoopBudget> {
    machine.states.get(current_state)?.poll.as_ref()?;
    // The state is its own destination: what is weighed is the self-loop the
    // spent attempts refuse, whichever edge then leaves it. §FS-rhei-errors.1.5
    spent_loop_budget(
        machine,
        metadata,
        &task.id,
        current_state,
        task.state.as_str(),
        current_state,
    )
}

/// How the edge a program's exit selected matched that exit.
///
/// A step-2 exact match is the program naming the route it wants, so the engine
/// records no subprocess-failure entry for it; a `"nonzero"` catch-all is the
/// program merely failing, and an edge carrying no `exit_code` at all — the
/// exit-`0` fall-through — is neither. A spent poll budget is its own answer:
/// the edge was selected regardless of what the program exited with, so the
/// exit named nothing whatever the rule declares. §FS-rhei-programs.3.2
#[derive(Debug, Clone)]
enum ExitCodeMatch {
    /// An integer or integer-array `exit_code:` condition: a **declared route**.
    Exact,
    /// A `"nonzero"` catch-all condition.
    Nonzero,
    /// The selected rule declares no `exit_code:` at all.
    None,
    /// The state's poll attempt budget was spent, so the engine selected this
    /// edge regardless of its `exit_code:` — whatever it declares, the exit did
    /// not choose it. It carries the budget as the selection measured it, so the
    /// account of the spent budget cannot exist without its figure.
    /// §FS-rhei-programs.3.2 §FS-rhei-run.5.1
    PollExhausted(SpentLoopBudget),
}

impl ExitCodeMatch {
    fn of(rule: &rhei_core::ast::TransitionRule) -> Self {
        if transition_has_exact_exit_code(rule) {
            Self::Exact
        } else if transition_is_nonzero_exit_code(rule) {
            Self::Nonzero
        } else {
            Self::None
        }
    }

    /// Whether this exit chose its own edge, and so owns the account of the
    /// outcome the engine would otherwise write. §FS-rhei-programs.3.2
    fn is_declared_route(&self) -> bool {
        matches!(self, Self::Exact)
    }
}

/// The edge a program's exit selects, and how it matched.
///
/// The two travel together because only the selection can answer the second
/// question: it is the one place a rule's `condition:` is evaluated, so an
/// exact edge its condition disqualified is a rule the machine still declares
/// and the exit still did not take. Asking the machine again afterwards would
/// read that exit as a declared route and drop the engine's account of a
/// genuine failure. §FS-rhei-programs.3.2
#[derive(Debug, Clone)]
struct ProgramExitRoute {
    to: String,
    matched: ExitCodeMatch,
    /// Which rule of `machine.transitions()` was chosen, by index.
    ///
    /// The `(from, to)` pair does not identify a rule: several rules may
    /// declare the same pair and differ only in their `exit_code:`, their
    /// `condition:`, or their callbacks. So the pair cannot be re-resolved
    /// afterwards to recover what fired — the selection has to say.
    ///
    /// It indexes the `transitions()` of the machine the selection was run
    /// against, so it means nothing apart from that machine: the caller hands
    /// the two to the apply step in one call, and the apply step accepts the
    /// index only for a rule that still declares the pair it is moving along.
    /// §FS-rhei-programs.3.2 §FS-rhei-transitions.4.4
    selected_rule: usize,
}

fn find_program_exit_transition(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
    exit_code: i32,
) -> MietteResult<Option<ProgramExitRoute>> {
    let applicable_exact_match_exists = exit_code != 0
        && machine
            .transitions()
            .iter()
            .filter(|rule| rule.from.0 == current_state)
            .filter(|rule| transition_has_exact_exit_code(rule))
            .filter(|rule| transition_matches_exit_code(rule, exit_code))
            .any(|rule| {
                program_transition_is_applicable(rule, machine, metadata, task, current_state)
            });

    let spent_budget = (exit_code != 0)
        .then(|| spent_poll_budget(machine, metadata, task, current_state))
        .flatten();

    // A non-zero exit selects only an exit_code-declaring rule — except a
    // poll state's exhaustion edge, the spec'd non-self-loop match once
    // attempts are spent. §FS-rhei-programs.3.2 §FS-rhei-programs.3.3 §FS-rhei-run.5.1
    let ordered_match = machine
        .transitions()
        .iter()
        .enumerate()
        .filter(|(_, rule)| rule.from.0 == current_state)
        .filter(|(_, rule)| {
            if exit_code == 0 {
                rule.exit_code.is_none() || transition_matches_exit_code(rule, exit_code)
            } else if rule.exit_code.is_none() {
                spent_budget.is_some() && rule.to.0 != current_state
            } else {
                transition_matches_exit_code(rule, exit_code)
            }
        });

    let mut applicable = ordered_match
        .filter(|(_, rule)| {
            !(applicable_exact_match_exists && transition_is_nonzero_exit_code(rule))
        })
        .filter(|(_, rule)| {
            program_transition_is_applicable(rule, machine, metadata, task, current_state)
        });
    let Some((first_index, first_rule)) = applicable.next() else {
        return Ok(None);
    };

    // While a poll budget lasts, a self-loop of the same specificity beats the exhaustion
    // edge whatever the declaration order. §FS-rhei-run.5.1 §FS-rhei-programs.8.2
    let polling_with_budget = spent_budget.is_none()
        && machine.states.get(current_state).is_some_and(|state| state.poll.is_some());
    let (selected_rule, rule) = if polling_with_budget
        && first_rule.exit_code.is_some()
        && first_rule.to.0 != current_state
    {
        let specificity = transition_is_nonzero_exit_code(first_rule);
        applicable
            .find(|(_, rule)| {
                rule.to.0 == current_state
                    && rule.exit_code.is_some()
                    && transition_is_nonzero_exit_code(rule) == specificity
            })
            .unwrap_or((first_index, first_rule))
    } else {
        (first_index, first_rule)
    };

    // Classified here, where the condition fired, not re-derived later.
    // An exhausted poll budget picks the edge on its own, so the exit
    // chose nothing and the engine still owes the account. §FS-rhei-programs.3.2
    let matched = match spent_budget {
        Some(budget) => ExitCodeMatch::PollExhausted(budget),
        None => ExitCodeMatch::of(rule),
    };
    Ok(Some(ProgramExitRoute {
        to: rule.to.0.clone(),
        matched,
        selected_rule,
    }))
}
