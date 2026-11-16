//! Healthy startup stalls must fit both the ordering harness and its effective
//! outer execution bounds. §FS-rhei-agent-visit-pairing.5.1

use super::*;

// §FS-rhei-agent-visit-pairing.5.1: port #739's real six-second interpreter stall.
#[test]
fn failed_own_record_survives_a_six_second_healthy_startup_stall_in_both_orders() {
    for order in ["c-first", "d-first"] {
        failure_support::check_failed_own_record_with(
            Some(order),
            include_str!("fixtures/agent_target_edit_startup_delay.py"),
            &|dir| {
                eprintln!("#739 {order} fixture: {}", dir.display());
                // Retain the cause alongside the scenario's unchanged record/journal assertions.
                for name in ["c", "d"] {
                    let log =
                        dir.join(format!("runtime/logs/task-plan.1-work-mock-mock-{name}-2.log"));
                    eprintln!(
                        "{name} interpreter log:\n{}",
                        fs::read_to_string(log).unwrap_or_default()
                    );
                }
            },
        );
    }
}

// §FS-rhei-agent-visit-pairing.5.1: profiles override the fallback; check every generated bound.
#[test]
fn target_edit_execution_bounds_all_use_the_named_fixture_margin() {
    let margin = super::super::helpers::FIXTURE_MARGIN;
    let machine: serde_yaml::Value =
        serde_yaml::from_str(&machine(r#"all_targets: ["mock:mock:c", "mock:mock:d"]"#))
            .expect("generated target-edit machine");
    let mut bounds = vec![(
        "states.work.agent_timeout".to_owned(),
        machine["states"]["work"]["agent_timeout"].as_str().map(str::to_owned),
    )];
    for with_models in [false, true] {
        let dir = unique_temp_dir("agent-target-edit-settings-margin");
        let agent = write_python_agent(&dir, "mock-agent.py", D_FAILS_AGENT);
        write_settings(&dir, &agent, with_models);
        let settings: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join(".agent-grounds/rhei/settings.json"))
                .expect("generated target-edit settings"),
        )
        .expect("parse generated settings");
        let profiles = settings["agents"].as_object().expect("generated agent profiles");
        assert!(!profiles.is_empty(), "settings must exercise an effective agent profile");
        bounds.push((
            format!("with_models={with_models}: defaults.agent_timeout"),
            settings["defaults"]["agent_timeout"].as_str().map(str::to_owned),
        ));
        bounds.extend(profiles.iter().map(|(name, profile)| {
            (
                format!("with_models={with_models}: agents.{name}.timeout"),
                profile["timeout"].as_str().map(str::to_owned),
            )
        }));
    }
    let wrong: Vec<_> = bounds
        .into_iter()
        .filter(|(_, bound)| bound.as_deref() != Some(margin))
        .map(|(path, bound)| format!("{path}: expected {margin} fixture margin, got {bound:?}"))
        .collect();
    assert!(wrong.is_empty(), "healthy target-edit starts can be preempted:\n{}", wrong.join("\n"));
}
