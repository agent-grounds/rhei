// The machine-only switch that delegates the two count ceilings, and the
// resolution it changes.
//
// The switch is read from the machine file alone, as a strict boolean, and
// refused by its presence in a project file; under it, a count key the project
// declares becomes the ceiling, while spend and the lifetime maximum stay the
// machine's whatever it says.

// §FS-rhei-budgets.2 §FS-rhei-budgets.2.1 §FS-rhei-agents.1.1.1

/// What [`merged_delegation`] read, and the directory that keeps its files.
type Delegation = (Result<RheiSettings, miette::Report>, PathBuf, PathBuf, tempfile::TempDir);

/// Merged settings from a machine file and, where `project` is given, a
/// project file in `home` beside the plan, with the machine and project paths
/// the loader opened.
fn merged_delegation(machine: &str, home: &str, project: Option<&str>) -> Delegation {
    let dir = tempfile::tempdir().expect("tmpdir");
    let plan_root = dir.path().join("plan");
    // One component at a time, as the loader joins it, so Windows paths compare equal.
    let project_file =
        home.split('/').fold(plan_root.clone(), |path, part| path.join(part)).join("settings.json");
    std::fs::create_dir_all(project_file.parent().expect("home dir")).expect("project home");
    if let Some(project) = project {
        std::fs::write(&project_file, project).expect("write project");
    }
    let temp_home = TempHome::new();
    let machine_file = home_dir().expect("home").join(".config/rhei/settings.json");
    std::fs::create_dir_all(machine_file.parent().expect("config dir")).expect("config dir");
    std::fs::write(&machine_file, machine).expect("write machine");
    let merged = load_merged_settings(&plan_root);
    drop(temp_home);
    (merged, machine_file, project_file, dir)
}

/// `false` delegates, `true` and omission do not, and the files are recorded
/// by the paths the loader opened. §FS-rhei-agents.1.1.1 §FS-rhei-budgets.8
#[test]
fn the_switch_is_read_from_the_machine_file_as_a_boolean() {
    for (defaults, delegated) in
        [(r#"{ "clamp_projects": false }"#, true), (r#"{ "clamp_projects": true }"#, false), ("{}", false)]
    {
        let (merged, machine, project, _dir) = merged_delegation(
            &format!(r#"{{ "defaults": {defaults} }}"#),
            ".agent-grounds/rhei",
            Some(r#"{ "defaults": { "transition_limit": 6 } }"#),
        );
        let settings = merged.unwrap_or_else(|err| panic!("{defaults} is lawful; got: {err:?}"));
        assert_eq!(settings.ceiling_policy.delegated, delegated, "{defaults}");
        assert_eq!(settings.ceiling_policy.files.machine.as_deref(), Some(machine.as_path()));
        assert_eq!(settings.ceiling_policy.files.project.as_deref(), Some(project.as_path()));
    }
}

/// A string, a number or `null` in the machine file is a settings error that
/// names the key. §FS-rhei-agents.1.1.1
#[test]
fn a_non_boolean_switch_in_the_machine_file_is_refused() {
    for value in [r#""false""#, "0", "null"] {
        let (merged, _, _, _dir) = merged_delegation(
            &format!(r#"{{ "defaults": {{ "clamp_projects": {value} }} }}"#),
            ".agent-grounds/rhei",
            None,
        );
        let message = format!("{:?}", merged.expect_err("a non-boolean is refused"));
        assert!(message.contains("clamp_projects"), "{value}: {message}");
    }
}

/// In either project home the switch is refused by its presence, whatever its
/// value, naming the file read and the machine file. §FS-rhei-agents.1.1.1
#[test]
fn the_switch_in_a_project_file_is_refused_by_its_presence() {
    for home in [".agent-grounds/rhei", ".agents/rhei"] {
        for value in ["true", "false", "null"] {
            let (merged, machine, project, _dir) = merged_delegation(
                "{}",
                home,
                Some(&format!(r#"{{ "defaults": {{ "clamp_projects": {value} }} }}"#)),
            );
            let message = merged.expect_err("the switch in a project is refused").to_string();
            assert!(
                message.contains(
                    "`defaults.clamp_projects` may only be set in the machine settings file"
                ),
                "{home} = {value}: {message}"
            );
            assert!(message.contains(&format!("found in:  {}", project.display())), "{message}");
            assert!(message.contains(&format!("set it in: {}", machine.display())), "{message}");
        }
    }
}

/// Under the switch, a declared count key's ceiling is the project's in either
/// direction, an undeclared one keeps the machine's, and spend and the lifetime
/// maximum are never delegated. §FS-rhei-budgets.2 §FS-rhei-budgets.2.1
#[test]
fn delegation_reaches_the_two_declared_count_keys_only() {
    let (merged, _, _, _dir) = merged_delegation(
        r#"{ "defaults": { "clamp_projects": false, "transition_limit": 80,
             "invocation_lifetime_max": 100, "spend_per_day": 5 } }"#,
        ".agent-grounds/rhei",
        Some(
            r#"{ "defaults": { "transition_limit": 40, "invocation_lifetime_max": 9000,
                 "spend_per_day": 50 } }"#,
        ),
    );
    let settings = merged.expect("lawful settings");
    let bounds = resolve_count_bounds(&settings, Some(60));

    assert_eq!(bounds.travel.effective, 40, "the project caps the profile's 60");
    assert_eq!(bounds.travel.limiter, rhei_core::budget::Limiter::Project);
    assert!(!bounds.per_day.delegated(), "an undeclared key keeps the machine's ceiling");
    assert_eq!(bounds.per_day.effective, rhei_core::budget::built_in::INVOCATIONS_PER_DAY);
    assert_eq!(bounds.lifetime_max.effective, 100, "the lifetime maximum stays the machine's");
    assert!(!bounds.lifetime_max.delegated());
    assert_eq!(bounds.spend.effective, 5 * rhei_core::money::MICRO, "spend stays the machine's");
    assert!(!bounds.spend.delegated());

    let lines = bounds.report_lines();
    assert_eq!(lines.len(), 6, "four bounds, one ceiling row, one policy row: {lines:#?}");
    assert!(lines[4].starts_with("transition_limit ceiling: 40 (project, "), "{lines:#?}");
    assert!(
        lines[5].starts_with(
            "count ceiling policy: delegated transition_limit by defaults.clamp_projects=false in "
        ),
        "{lines:#?}"
    );
}

/// Without the switch the same files resolve exactly as before: the machine
/// is the ceiling and no delegation row is reported. §FS-rhei-budgets.2
#[test]
fn without_the_switch_the_machine_stays_the_ceiling() {
    let (merged, _, _, _dir) = merged_delegation(
        r#"{ "defaults": { "transition_limit": 3 } }"#,
        ".agent-grounds/rhei",
        Some(r#"{ "defaults": { "transition_limit": 6 } }"#),
    );
    let bounds = resolve_count_bounds(&merged.expect("lawful settings"), None);
    assert_eq!(bounds.travel.effective, 3);
    assert_eq!(bounds.travel.requested, Some(6));
    assert_eq!(bounds.report_lines().len(), 4);
}
