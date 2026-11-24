// Leaving a gate whose brief asks authored questions by any human verb but
// `rhei gate answer`: the live dashboard's release is refused as
// `rhei transition` is, and `rhei complete` names the commands that answer it.

// §FS-rhei-gate-brief.8

/// A plan whose one ticket waits at `review`, a gate that asks one static
/// question, beside the states file that declares it.
fn briefed_gate_plan(dir: &Path) -> (PathBuf, PathBuf, &'static str) {
    let plan = dir.join("plan.rhei.md");
    let states = dir.join("states.yaml");
    fs::write(&plan, "# Rhei: Gate\n\n## Tasks\n\n### Task 1: Review\n**State:** review\n\nReview.\n")
        .expect("write plan");
    let yaml = "name: test\nversion: 1\nstates:\n  review:\n    description: review\n    gating: true\n    \
                gate:\n      questions:\n        - { id: approve, kind: boolean, prompt: \"Ship it?\" }\n      \
                routes:\n        - { to: completed, when: { approve: true } }\n        - { to: rework }\n  \
                rework:\n    description: back to work\n  completed:\n    description: done\n    final: true\n\
                transitions:\n  - { from: review, to: rework }\n  - { from: review, to: completed }\n  \
                - { from: rework, to: completed }\n";
    fs::write(&states, yaml).expect("write states");
    (plan, states, yaml)
}

/// The dashboard's release takes `rhei transition`'s refusal at a briefed gate,
/// whichever choice was made, and the reason it shows names the gate's own
/// commands on one line. §FS-rhei-viz.5.1 §FS-rhei-transition-cmd.3.5
#[test]
fn the_dashboard_cannot_release_a_gate_whose_brief_asks_questions() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let (plan, states, yaml) = briefed_gate_plan(dir.path());
    let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("machine");
    let callback_paths = resolve_callback_paths(Some(&states), &plan).expect("callbacks");

    for to in ["completed", "rework"] {
        let err = transition_dashboard_gate(&plan, &machine, &callback_paths, "plan.1", "review", to, Some("done"), true)
            .expect_err("a briefed gate is answered, not released");
        assert!(err.to_string().contains("waits at gate 'review', which asks 1 required question"), "{err}");
        let shown = gate_release_refusal(&err);
        assert!(!shown.contains('\n'), "one line: {shown}");
        assert!(shown.contains("gate show") && shown.contains("gate answer"), "{shown}");
        assert!(shown.contains("--from review --answers <file|->"), "{shown}");
    }
    let untouched = fs::read_to_string(&plan).expect("read plan");
    assert!(untouched.contains("**State:** review"), "{untouched}");
    assert!(!dir.path().join("runtime/state-transitions.log").exists(), "nothing moved");
}

/// `rhei complete` never leaves a gate; at a briefed one it says how the gate
/// is left instead of naming the `rhei transition` that would be refused too.
/// §FS-rhei-complete.4
#[test]
fn complete_at_a_briefed_gate_names_gate_show_and_gate_answer() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let (plan, states, _) = briefed_gate_plan(dir.path());

    let err = complete_command(&plan, &[], Some(&states), "1", "done", true)
        .expect_err("completing from a gate must fail");
    assert!(err.to_string().contains("cannot be completed from gating state 'review'"), "{err}");
    assert!(err.to_string().contains("asks 1 required question; answer it instead"), "{err}");
    let help = err.help().map(|help| help.to_string()).unwrap_or_default();
    assert!(help.contains("read the brief with: rhei gate show"), "{help}");
    assert!(help.contains("answer it with: rhei gate answer"), "{help}");
    assert!(!help.contains("rhei transition"), "{help}");
}
