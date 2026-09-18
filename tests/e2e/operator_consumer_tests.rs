//! Real CLI consumers of a pair written by the attended command. §FS-rhei-complete.3.1

use super::operator_attended_tests::attended;
use super::operator_force_support::*;
use super::*;

/// The operator's source handoff is consumed by one isolated capture agent. §FS-rhei-transition-cmd.6
fn consumer_fixture(prefix: &str) -> ForceFixture {
    let machine = FORCE_MACHINE
        .replace("    description: Human decision", "    description: Human decision\n    outputs:\n      - name: ruling\n        kind: handoff\n        path: runtime/ruling.md")
        .replace("    description: Work", "    description: Work\n    agent: capture\n    agent_timeout: 30s\n    handoff:\n      inherit:\n        - from: transition.previous\n          required: true")
        .replace("    to: cancelled", "    to: completed");
    let fixture = force_fixture(prefix, GATE_PLAN, &machine);
    let agent = write_python_agent(
        &fixture.dir,
        "capture.py",
        r#"
write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'consumer-prompt.md', agent_prompt())
result('## Result\n\nCompleted the isolated consumer scenario.\n')
"#,
    );
    let settings = fixture.dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).unwrap();
    fs::write(settings.join("settings.json"), format!(
        r#"{{"defaults":{{"agent":"capture","agent_timeout":"30s"}},"agents":{{"capture":{{"command":{},"stdin_prompt":true,"timeout":"30s"}}}}}}"#,
        fixture_command(&agent))).unwrap();
    fs::create_dir_all(fixture.dir.join("runtime")).unwrap();
    fs::write(fixture.dir.join("runtime/ruling.md"), "The operator's reviewed handoff.").unwrap();
    let (success, transcript) = attended(
        &fixture,
        &[
            "--state-machine",
            fixture.machine.to_str().unwrap(),
            "transition",
            fixture.plan.to_str().unwrap(),
            "--task",
            "1",
            "--from",
            "human-gate",
            "--to",
            "implement",
            "--force",
            "--reason",
            "repair route",
        ],
        "force plan.1 human-gate -> implement\r\n",
    );
    assert!(success, "{transcript}");
    fixture
}

/// Visualization, Previous Visits and inherited handoff each consume one movement. §FS-rhei-complete.3.1
#[test]
fn operator_cli_viz_prompt_and_handoff_consume_the_writer_pair_once() {
    let fixture = consumer_fixture("operator-consumer-history");
    let html = fixture.dir.join("view.html");
    assert_success(&run_cli(
        "viz",
        &fixture.plan,
        &fixture.machine,
        &["--output", html.to_str().unwrap()],
    ));
    let html = fs::read_to_string(html).unwrap();
    assert_eq!(
        html.matches("\"forced_reason\":\"repair route\"").count(),
        1,
        "one exceptional history entry"
    );
    assert!(!html.contains("!force-v1"), "metadata is not a second displayed state");
    assert_success(&run_cli(
        "run",
        &fixture.plan,
        &fixture.machine,
        &["--no-callbacks", "--no-tui"],
    ));
    let prompt = fs::read_to_string(fixture.dir.join("runtime/consumer-prompt.md")).unwrap();
    assert_eq!(
        prompt
            .matches("Trail for this task: human-gate → implement (this visit, visit 1).")
            .count(),
        1,
        "{prompt}"
    );
    assert_eq!(prompt.matches("## Handoff from human-gate").count(), 1, "{prompt}");
    assert!(prompt.contains("The operator's reviewed handoff."), "{prompt}");
    assert!(!prompt.contains("!force-v1"));
}

/// Narrowed reset rewinds authored state and keeps the sibling's complete pair. §FS-rhei-reset.2.2
#[test]
fn operator_cli_narrowed_reset_preserves_other_pairs_and_authored_state() {
    let fixture = consumer_fixture("operator-consumer-reset");
    fs::write(fixture.dir.join("index.panta.md"), "# Panta: Recovery consumers\n").unwrap();
    let sibling = fixture.dir.join("other.rhei.md");
    fs::write(&sibling, GATE_PLAN.replace("human-gate", "implement")).unwrap();
    let ledger_path = fixture.dir.join("runtime/state-transitions.log");
    let original = fs::read_to_string(&ledger_path).unwrap();
    let mut audit =
        rhei_core::transition_history::parse(&original).unwrap()[0].audit.clone().unwrap();
    audit.task_id = "other.1".into();
    audit.recovery_id = "018f0000-0000-7000-8000-000000000002".into();
    let (metadata, movement) = audit.pair().unwrap();
    let sibling_pair = format!("{metadata}{movement}");
    fs::write(&ledger_path, format!("{original}{sibling_pair}")).unwrap();
    let sibling_before = fs::read(&sibling).unwrap();
    assert_success(&run_cli("reset", &fixture.dir, &fixture.machine, &["--rhei", "plan", "--yes"]));
    assert!(fs::read_to_string(&fixture.plan).unwrap().contains("**State:** human-gate"));
    assert_eq!(fs::read(&sibling).unwrap(), sibling_before);
    assert_eq!(fs::read_to_string(&ledger_path).unwrap(), sibling_pair);
}

/// Known-pair corruption is refused by real read, reset and agent-launch paths. §FS-rhei-complete.3.1
#[test]
fn operator_cli_consumers_refuse_a_mismatching_pair() {
    for command in ["viz", "reset", "run"] {
        let fixture = consumer_fixture(&format!("operator-consumer-corrupt-{command}"));
        let path = fixture.dir.join("runtime/state-transitions.log");
        let corrupt = fs::read_to_string(&path)
            .unwrap()
            .replace("human-gate@implement", "human-gate@completed");
        fs::write(&path, &corrupt).unwrap();
        let plan_before = fs::read(&fixture.plan).unwrap();
        let extra: &[&str] = match command {
            "reset" => &["--dry-run"],
            "run" => &["--no-callbacks", "--no-tui"],
            _ => &[],
        };
        let run = run_cli(command, &fixture.plan, &fixture.machine, extra);
        assert!(!run.status.success(), "{command}: {} {}", run.stdout, run.stderr);
        assert!(
            format!("{}{}", run.stdout, run.stderr).contains("force"),
            "missing corruption diagnostic"
        );
        assert_eq!(fs::read(&fixture.plan).unwrap(), plan_before);
        assert_eq!(fs::read_to_string(&path).unwrap(), corrupt);
        assert!(!fixture.dir.join("runtime/consumer-prompt.md").exists());
    }
}
