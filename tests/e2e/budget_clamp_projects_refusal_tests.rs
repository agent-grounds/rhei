//! The switch that delegates the count ceilings belongs to the paying machine
//! alone: anywhere else it is refused by its presence, whatever its value, and
//! in the machine file it is a strict boolean.

// §FS-rhei-agents.1.1.1 §FS-rhei-budgets.2.1

use std::path::Path;

use super::budget_delegated_support::*;
use super::budget_support::*;

/// Where in a state machine the switch is written, as an edit of the machine.
type Place = fn(&str, &str) -> String;

/// Validate a workspace that is expected to be refused for carrying the switch
/// where only the machine may, and check the refusal names the key, where it
/// was found and where it belongs. §FS-rhei-agents.1.1.1
fn assert_refused_outside_the_machine(
    dir: &Path,
    plan: &Path,
    machine: &Path,
    found: &str,
    case: &str,
) {
    let result = run_at("validate", plan, machine, None, &[]);
    let text = combined(&result);
    assert!(
        !result.status.success(),
        "{case}: the switch outside the machine file is refused; got:\n{text}"
    );
    assert!(
        text.contains("`defaults.clamp_projects` may only be set in the machine settings file"),
        "{case}: the refusal names the key; got:\n{text}"
    );
    assert!(
        text.contains(found),
        "{case}: the refusal names where it was found ({found}); got:\n{text}"
    );
    assert!(
        spellings(&machine_file(dir)).iter().any(|m| text.contains(&format!("set it in: {m}"))),
        "{case}: the refusal names the machine file where it belongs; got:\n{text}"
    );
}

/// A repository cannot opt itself out: the switch in either project home is
/// refused by its presence, whatever its value.
// §FS-rhei-agents.1.1.1
#[test]
fn the_switch_in_either_project_settings_home_is_refused_whatever_its_value() {
    for home in [".agent-grounds/rhei", ".agents/rhei"] {
        for value in ["true", "false", "null"] {
            let (dir, plan, machine) = workspace(
                "budget-delegated-refuse-project",
                &profile_machine(None),
                "",
                home,
                &format!(r#"{{ "defaults": {{ "clamp_projects": {value} }} }}"#),
            );
            let found = format!("{home}/settings.json");
            assert_refused_outside_the_machine(
                &dir,
                &plan,
                &machine,
                &found,
                &format!("{home} = {value}"),
            );
        }
    }
}

/// Nor can a plan: the switch at a state machine's root, in its root
/// `defaults`, on a profile or in a profile's `defaults` is refused before any
/// deserializer can drop it as an unknown key.
// §FS-rhei-agents.1.1.1
#[test]
fn the_switch_anywhere_in_a_state_machine_is_refused_whatever_its_value() {
    let places: [(&str, Place); 4] = [
        ("clamp_projects", |m, v| {
            m.replacen("version: 1\n", &format!("version: 1\nclamp_projects: {v}\n"), 1)
        }),
        ("defaults.clamp_projects", |m, v| {
            m.replacen(
                "version: 1\n",
                &format!("version: 1\ndefaults:\n  clamp_projects: {v}\n"),
                1,
            )
        }),
        ("profiles.loop.clamp_projects", |m, v| {
            m.replacen(
                "    initial: work\n",
                &format!("    clamp_projects: {v}\n    initial: work\n"),
                1,
            )
        }),
        ("profiles.loop.defaults.clamp_projects", |m, v| {
            m.replacen(
                "    initial: work\n",
                &format!("    defaults:\n      clamp_projects: {v}\n    initial: work\n"),
                1,
            )
        }),
    ];
    for (field, place) in places {
        for value in ["true", "false", "null"] {
            let states = place(&profile_machine(None), value);
            let (dir, plan, machine) = setup("budget-delegated-refuse-states", &states, DELEGATES);
            assert_refused_outside_the_machine(
                &dir,
                &plan,
                &machine,
                field,
                &format!("{field} = {value}"),
            );
        }
    }
}

/// In the machine file, the one place it is lawful, the switch is a strict
/// boolean: a string, a number or `null` is a settings error naming the key.
// §FS-rhei-agents.1.1.1 §FS-rhei-budgets.2.1
#[test]
fn the_switch_in_the_machine_file_must_be_a_boolean() {
    for value in [r#""false""#, "0", "null"] {
        let (_dir, plan, machine) = setup(
            "budget-delegated-machine-type",
            &profile_machine(None),
            &format!(r#", "clamp_projects": {value}"#),
        );
        let result = run_at("validate", &plan, &machine, None, &[]);
        let text = combined(&result);
        assert!(!result.status.success(), "clamp_projects = {value} is a type error; got:\n{text}");
        assert!(text.contains("clamp_projects"), "the error names the key; got:\n{text}");
    }
}
