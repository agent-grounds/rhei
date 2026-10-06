// Resolving the four bounds in force, and finding the project an account
// belongs to.
//
// Its own part because resolution is a *settings* question and admission is a
// *ledger* one: this file knows the tiers and the ceiling, and the module next
// door knows the receipts. Keeping the clamp here is what makes it a step over
// the existing `built_in -> global -> project` chain rather than a second
// precedence system.

// §AR-source-file-size.3 §FS-rhei-budgets.2 §AR-neural-admission.2

use rhei_core::budget::{built_in, Bound, BoundSource};

/// The four bounds in force, each with the value and the provenance every
/// surface prints. §FS-rhei-budgets.2.3
#[derive(Clone, Debug, PartialEq, Eq)]
struct CountBounds {
    travel: Bound,
    per_day: Bound,
    lifetime_max: Bound,
    /// The one whose number is money rather than a count, reported like the
    /// other three as the bare number its key takes. §FS-rhei-budgets.2.1
    spend: Bound,
}

impl CountBounds {
    /// The lines `rhei validate` and `rhei budget show` report, in dimension
    /// order, followed where the machine delegated a count ceiling by one
    /// ceiling line per delegated key and the policy line.
    /// §FS-rhei-validate.4 §FS-rhei-budgets.2.3
    fn report_lines(&self) -> Vec<String> {
        let mut lines = vec![
            self.travel.report_line(),
            self.per_day.report_line(),
            self.lifetime_max.report_line(),
            self.spend.report_line(),
        ];
        lines.extend(self.counts().filter_map(Bound::ceiling_line));
        lines.extend(rhei_core::budget::ceiling_policy_line(self.counts()));
        lines
    }

    /// The two count dimensions, the only ones a machine can delegate.
    /// §FS-rhei-budgets.2
    fn counts(&self) -> impl Iterator<Item = &Bound> {
        [&self.travel, &self.per_day].into_iter()
    }

    fn effective(&self) -> rhei_core::budget::EffectiveBounds {
        rhei_core::budget::EffectiveBounds {
            transition_limit: self.travel.effective,
            invocations_per_day: self.per_day.effective,
            spend_per_day: self.spend.effective,
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
    let policy = &settings.ceiling_policy;
    let requested = |plan: Option<u64>, project: Option<u64>| {
        plan.map(|value| (value, BoundSource::Plan))
            .or_else(|| project.map(|value| (value, BoundSource::Project)))
    };
    // The project's ceiling where the machine delegated and the project declares
    // the key, one key at a time; spend and lifetime never reach here. §FS-rhei-budgets.2
    let count = |key, built_in, machine: Option<u64>, plan: Option<u64>, declared: Option<u64>| {
        let bound = match declared.filter(|_| policy.delegated) {
            Some(ceiling) => Bound::resolve_delegated(key, ceiling, requested(plan, declared)),
            None => Bound::resolve(key, built_in, machine, requested(plan, declared)),
        };
        bound.with_files(policy.files.clone())
    };
    CountBounds {
        travel: count(
            "transition_limit",
            built_in::TRANSITION_LIMIT,
            machine.transition_limit,
            profile_limit,
            project.transition_limit,
        ),
        // Neither is declarable on a plan: a plan that could raise the day's
        // starts would be raising the cap of whichever machine ran it.
        // §FS-rhei-budgets.2.1
        per_day: count(
            "invocations_per_day",
            built_in::INVOCATIONS_PER_DAY,
            machine.invocations_per_day,
            None,
            project.invocations_per_day,
        ),
        lifetime_max: Bound::resolve(
            "invocation_lifetime_max",
            built_in::INVOCATION_LIFETIME_MAX,
            machine.invocation_lifetime_max,
            requested(None, project.invocation_lifetime_max),
        )
        .with_files(policy.files.clone()),
        // Machine and project tiers only: a plan that could raise a day's
        // spend would be raising the cap of whichever machine paid for it.
        // §FS-rhei-budgets.2.1
        spend: Bound::resolve_money(
            "spend_per_day",
            built_in::SPEND_PER_DAY,
            machine.spend_per_day,
            requested(None, project.spend_per_day),
        )
        .with_files(policy.files.clone()),
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
/// case reports the built-in four, and a plan whose machine declares a profile
/// limit reports that one. §FS-rhei-validate.4
fn plan_count_bounds_with(settings: &RheiSettings, declared: Option<u64>) -> CountBounds {
    // Several profiles mean several travel bounds; validation reports the
    // root node's, which is what bounds every node declaring nothing. A node
    // with its own is reported by the run. §FS-rhei-budgets.2.2
    resolve_count_bounds(settings, declared)
}

/// The project an account belongs to: the Panta project that discovers the
/// plan as a member, or the plan's own execution root when it is a bare rhei.
///
/// Membership is read as the loader reads it (§FS-rhei-panta.6): an execution
/// root that is itself a project is its own, and a Directory Workspace or the
/// `basin/` its parent discovers belongs to that parent. Anything else - a plan
/// under the project's `runtime/`, in a grouping folder, in a scratch directory
/// - is a bare rhei however deep inside a project it sits.
///
/// A bare rhei is the single rhei of its implicit project, so it has an account
/// of its own; a member never does, because a project that could add a rhei to
/// buy capacity would not be bounded at all.
/// §FS-rhei-budgets.1 §FS-rhei-budgets.5.1
fn budget_project_root(workspace_root: &Path) -> PathBuf {
    if rhei_core::workspace::is_panta_project(workspace_root) {
        return workspace_root.to_path_buf();
    }
    // A root spelled `.` names no entry of its own, so ask about where it is.
    let entry = std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());

    // Only the project that discovers the root charges it; an enclosing one that
    // does not is no account of this plan's. §FS-rhei-budgets.5.1
    match rhei_core::workspace::panta_member(&entry) {
        Some((project, _)) => project,
        None => workspace_root.to_path_buf(),
    }
}

/// The four bound lines `rhei validate` reports, in dimension order.
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
