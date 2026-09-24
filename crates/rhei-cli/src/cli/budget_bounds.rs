// Resolving the three count bounds, and finding the project an account belongs
// to.
//
// Its own part because resolution is a *settings* question and admission is a
// *ledger* one: this file knows the tiers and the ceiling, and the module next
// door knows the receipts. Keeping the clamp here is what makes it a step over
// the existing `built_in -> global -> project` chain rather than a second
// precedence system.

// §AR-source-file-size.3 §FS-rhei-budgets.2 §AR-neural-admission.2

use rhei_core::budget::{built_in, Bound, BoundSource};

/// The three bounds in force, each with the value and the provenance every
/// surface prints. §FS-rhei-budgets.2.3
#[derive(Clone, Debug, PartialEq, Eq)]
struct CountBounds {
    travel: Bound,
    per_day: Bound,
    lifetime_max: Bound,
}

impl CountBounds {
    /// The lines `rhei validate` reports and the run's bounds section renders,
    /// in dimension order. §FS-rhei-validate.4
    fn report_lines(&self) -> Vec<String> {
        vec![self.travel.report_line(), self.per_day.report_line(), self.lifetime_max.report_line()]
    }

    fn effective(&self) -> rhei_core::budget::EffectiveBounds {
        rhei_core::budget::EffectiveBounds {
            transition_limit: self.travel.effective,
            invocations_per_day: self.per_day.effective,
        }
    }
}

/// Resolve every bound in force for one node.
///
/// `profile_limit` is what the plan's own state machine declares for the node
/// being admitted — a `profiles.<name>.transition_limit`. From the operator's
/// side the plan is what asked, whichever of its files the number sits in.
/// §FS-rhei-budgets.2 §FS-rhei-budgets.2.2
fn resolve_count_bounds(settings: &RheiSettings, profile_limit: Option<u64>) -> CountBounds {
    let machine = settings.machine_bounds;
    let project = settings.project_bounds;
    let requested = |plan: Option<u64>, project: Option<u64>| {
        plan.map(|value| (value, BoundSource::Plan))
            .or_else(|| project.map(|value| (value, BoundSource::Project)))
    };
    CountBounds {
        travel: Bound::resolve(
            "transition_limit",
            built_in::TRANSITION_LIMIT,
            machine.transition_limit,
            requested(profile_limit, project.transition_limit),
        ),
        // Neither is declarable on a plan: a plan that could raise the day's
        // starts would be raising the cap of whichever machine ran it.
        // §FS-rhei-budgets.2.1
        per_day: Bound::resolve(
            "invocations_per_day",
            built_in::INVOCATIONS_PER_DAY,
            machine.invocations_per_day,
            requested(None, project.invocations_per_day),
        ),
        lifetime_max: Bound::resolve(
            "invocation_lifetime_max",
            built_in::INVOCATION_LIFETIME_MAX,
            machine.invocation_lifetime_max,
            requested(None, project.invocation_lifetime_max),
        ),
    }
}

/// The travel bound one ticket's node resolves: its profile's declared value
/// where it has one, and the settings chain where it does not.
///
/// Every node resolves a profile and therefore a travel bound, including the
/// virtual project root. §FS-rhei-budgets.2.2
fn node_transition_limit(
    machine: &rhei_validator::StateMachine,
    task: Option<&rhei_core::ast::Task>,
) -> Option<u64> {
    match task {
        Some(task) => machine
            .profile_for_node(&task.kind, task.profile_level())
            .and_then(|profile| profile.transition_limit),
        None => machine.root_profile().and_then(|profile| profile.transition_limit),
    }
}

/// The bounds a plan reports before anything is spent: the declaration-free
/// case reports the built-in three, and a plan whose machine declares a profile
/// limit reports that one. §FS-rhei-validate.4
fn plan_count_bounds_with(settings: &RheiSettings, declared: Option<u64>) -> CountBounds {
    // Several profiles mean several travel bounds; validation reports the
    // root node's, which is what bounds every node declaring nothing. A node
    // with its own is reported by the run. §FS-rhei-budgets.2.2
    resolve_count_bounds(settings, declared)
}

/// The project an account belongs to: the Panta project a plan is a member of,
/// or the plan's own execution root when it is a bare rhei.
///
/// A bare rhei is the single rhei of its implicit project, so it has an account
/// of its own; a member never does, because a project that could add a rhei to
/// buy capacity would not be bounded at all.
/// §FS-rhei-budgets.1 §FS-rhei-budgets.5.1
fn budget_project_root(workspace_root: &Path) -> PathBuf {
    let mut candidate = workspace_root;
    loop {
        if rhei_core::workspace::is_panta_project(candidate) {
            return candidate.to_path_buf();
        }
        match candidate.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => candidate = parent,
            _ => return workspace_root.to_path_buf(),
        }
    }
}

/// The three count-bound lines `rhei validate` reports, in dimension order.
///
/// Resolution failures are silent here on purpose: validation has already
/// succeeded, and a settings file this pass could not re-read is a diagnostic
/// the pass itself owes rather than one to invent in the report.
/// §FS-rhei-validate.4
fn validated_bound_lines(input: &Path, state_machine: Option<&Path>) -> Vec<String> {
    let workspace_root = execution_workspace_root(input);
    let Ok(settings) = load_merged_settings(&workspace_root) else { return Vec::new() };
    let declared = load_plan_for_validation(input)
        .ok()
        .and_then(|loaded| resolve_state_machines_for_loaded_plan(input, &loaded, state_machine).ok())
        .map(|resolved| resolved.validator_set().default)
        .and_then(|machine| node_transition_limit(&machine, None));
    plan_count_bounds_with(&settings, declared).report_lines()
}
