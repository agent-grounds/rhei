/// Execute the `validate` subcommand once or in watch mode.
fn validate_command(input: &Path, state_machine: Option<&Path>, watch: bool) -> MietteResult<()> {
    if watch {
        watch_validation_command(input, state_machine)
    } else {
        run_validation_once(input, state_machine)
    }
}

/// Parse a plan, load the selected states, and print validation results.
fn run_validation_once(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    let warnings = validation_warnings_or_error(input, state_machine)?;
    // All four, before any conditional warning: a bound nobody sees before
    // it is spent is not visible. Resolved and reported only — nothing here
    // locks, appends, or debits. §FS-rhei-validate.4 §FS-rhei-budgets.6.3
    let mut reported = validated_bound_lines(input, state_machine);
    // About the spend bound just reported, so before every other conditional
    // warning. §FS-rhei-validate.4.6
    reported.extend(validated_price_book_coverage(input, state_machine));
    reported.extend(warnings);
    print_validation_report(&reported);
    Ok(())
}

/// One whole validation pass, before anything decides what to do with it.
///
/// `rhei validate` collapses this straight into a report and a set of warnings,
/// but a command that validates its *own* write compares two passes to tell the
/// errors it introduced from the ones it found — which needs the error strings
/// themselves, not a rendered diagnostic.
// §FS-rhei-new.5.2
struct ValidationPass {
    errors: Vec<String>,
    warnings: Vec<String>,
    /// Guidance the errors carry without owning: the project-global lists a
    /// create elsewhere can change, kept out of the error text so the pre/post
    /// diff stays stable. §FS-rhei-new.5.2
    help: Vec<String>,
    /// Every state-machine source the pass resolved, for an error report to
    /// attribute exactly. §FS-rhei-validate.6
    state_machine_sources: Vec<ValidationMachineSource>,
    /// Reporting only; never substitutes for effective sources. §FS-rhei-validate.6.1
    override_context: Option<String>,
}

/// Run the whole validation pass, failing on the first error report and
/// returning its warnings otherwise.
///
/// Split out so a command that validates its *own* write can reuse the exact
/// pass `rhei validate` runs without its output — `rhei new` checks the project
/// still loads before it reports success.
// §FS-rhei-new.5.1
fn validation_warnings_or_error(
    input: &Path,
    state_machine: Option<&Path>,
) -> MietteResult<Vec<String>> {
    let pass = validation_pass(input, state_machine)?;
    // Both ordinary and watch validation report once, including failed passes. §FS-rhei-validate.6.1
    if let Some(context) = &pass.override_context {
        println!("{context}");
    }
    if !pass.errors.is_empty() {
        return Err(validation_report(
            input,
            &pass.state_machine_sources,
            &pass.errors,
            &pass.help,
        ));
    }
    Ok(pass.warnings)
}

/// The validation pass itself, reported as data. §FS-rhei-new.5.2
fn validation_pass(input: &Path, state_machine: Option<&Path>) -> MietteResult<ValidationPass> {
    let loaded = load_plan_for_validation(input)?;

    validation_pass_for_loaded(input, state_machine, loaded, None)
}

/// Validate a preloaded graph through the same machine, link, settings, and
/// snapshot checks as `rhei validate`. §FS-rhei-templates.6.1.2
fn validation_pass_for_loaded(
    input: &Path,
    state_machine: Option<&Path>,
    loaded: LoadedPlan,
    settings_root: Option<&Path>,
) -> MietteResult<ValidationPass> {

    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine)?;
    let machines = resolved.validator_set();
    let normalized_input = normalize_workspace_input(input);
    // A directory input is a Directory Workspace (or a Panta project, whose
    // rheis carry their own kind); a file is a Single-File Plan. §FS-rhei-plan-language.3.6
    let base = if normalized_input.is_dir() {
        rhei_validator::LinkBase::workspace_root(normalized_input.as_path())
    } else {
        rhei_validator::LinkBase::plan_directory(normalized_input.parent().unwrap_or(Path::new(".")))
    };
    let mut report = if loaded.is_panta_project() {
        // Panta task links validate against each ticket's owning rhei root. §AR-rhei-panta.5
        // The project directory is no rhei, so its manifest's links are not contained.
        let bases = rhei_validator::LinkBases {
            default: rhei_validator::LinkBase::plan_directory(base.dir),
            tasks: &loaded.task_roots,
            sections: &loaded.content_section_roots,
            workspace_roots: &loaded.workspace_roots,
        };
        rhei_validator::validate_with_machine_set_and_link_bases(&loaded.rhei, &machines, &bases)
    } else {
        let mut report = rhei_validator::Validator::with_machines(machines.clone())
            .validate_with_base(&loaded.rhei, Some(base));
        report.warnings.dedup();
        report
    };
    let workspace_root = execution_workspace_root(input);
    let settings = load_merged_settings(settings_root.unwrap_or(&workspace_root))?;
    report.errors.extend(validate_loaded_exclusions(&loaded, &resolved, input, &settings));
    report
        .errors
        .extend(validate_plan_settings_references(&loaded.rhei, &machines, &settings));
    // `rhei validate` resolves nothing at runtime, so it judges the plan as it
    // would run with agents. §FS-rhei-validate.4
    report.errors.extend(validate_orchestrator_timeouts(
        &loaded.rhei,
        &machines,
        &settings,
        &default_run_options(),
    ));
    report.errors.extend(validate_snapshot_plan_context(&loaded, &resolved));
    report.warnings.extend(snapshot_orphan_validation_warnings(
        &workspace_root,
        &loaded,
        &resolved,
        &settings,
    )?);
    // §FS-rhei-panta.6: an empty project is valid, but say discovery found
    // nothing — a misnamed or misplaced plan is otherwise silently invisible
    // behind a green validation.
    if loaded.is_panta_project() && loaded.rhei_ids.is_empty() {
        report.warnings.push(format!(
            "the project holds no rheis: discovery looks only for `*.rhei.md` files and \
             workspace directories placed directly next to index.panta.md — {}",
            add_a_rhei_hint()
        ));
    }
    // An empty rhei is valid, but a mistyped `tasks/` looks identical to a
    // deliberately empty one — name it rather than let it pass unremarked.
    // §FS-rhei-plan-language.1.2
    report.warnings.extend(empty_rhei_warnings(&loaded));
    report.warnings.extend(ignored_member_settings_warnings(input, &loaded));
    // A concurrent state the file layout serializes is valid, but `rhei run`
    // would drop --parallel to 1 under it — name it before the round is spent.
    // §FS-rhei-validate.4.5
    report.warnings.extend(serialized_concurrent_state_warnings(input, &loaded, &machines));

    report.help.dedup();
    Ok(ValidationPass {
        errors: report.errors,
        warnings: report.warnings,
        help: report.help,
        state_machine_sources: resolved.validation_sources(&loaded.rhei_ids),
        override_context: validation_override_context(input, &loaded, state_machine),
    })
}

/// Print success output and any non-fatal validation warnings.
fn print_validation_report(warnings: &[String]) {
    println!("Validation succeeded");
    for warning in warnings {
        println!("warning: {warning}");
    }
}
