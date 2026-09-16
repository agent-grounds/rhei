//! §FS-rhei-library.4–5 §FS-rhei-states.1.4 §FS-rhei-transitions.4.6
use super::*;

fn cancellable(name: &str) -> Block {
    leaf(
        name,
        r#"name: cancel
version: 1
states: {work: {}, done: {final: true, gating: true}, canceled: {final: true}}
transitions: [{from: work, to: done}, {from: '*', to: canceled}]
"#,
        "### Task job: Work\n**State:** work\n",
    )
}

#[test]
fn mounted_escapes_keep_owner_scope_and_consumed_gate_cancellation() {
    let compiled = group(vec![("a", cancellable("a")), ("b", cancellable("b"))]).compile().unwrap();
    let machine = &compiled.fragment.machine;
    let escape = machine.transitions.iter().find(|r| r.to.0 == "m1_a__canceled").unwrap();
    assert_eq!(escape.from.0, "*");
    assert!(machine.is_cancellation("m1_a__canceled"));
    assert!(machine.transition_matches_source(escape, "m1_a__work"));
    assert!(machine.transition_matches_source(escape, "m1_a__done"));
    assert!(!machine.transition_matches_source(escape, "m1_b__work"));
    assert!(!machine.transition_matches_source(escape, "m1_a__canceled"));
    assert!(machine.states["m1_a__done"].gating);
    assert!(machine.transitions.iter().any(|r| r.from.0 == "m1_a__done" && r.to.0 == "m1_b__work"));
}

#[test]
fn source_sets_and_cancellation_roles_are_validated_before_lowering() {
    for rule in [
        "{from: work, sources: [work], to: done}",
        "{from: '*', sources: [ghost], to: done}",
        "{from: '*', sources: [work, work], to: done}",
    ] {
        let mut block = simple("bad");
        block.local.as_mut().unwrap().machine.transitions =
            serde_yaml::from_str(&format!("[{rule}]")).unwrap();
        assert!(block.compile().unwrap_err().contains("source"));
    }
    for state in ["{role: cancellation}", "{final: true, role: failed}"] {
        let mut block = simple("bad");
        block
            .local
            .as_mut()
            .unwrap()
            .machine
            .states
            .insert("abandoned".into(), serde_yaml::from_str(state).unwrap());
        assert!(block.compile().unwrap_err().contains("role"));
    }
    let mut block = cancellable("limited");
    block.local.as_mut().unwrap().machine.transitions[1].sources = Some(vec!["work".into()]);
    let compiled = group(vec![("a", block)]).compile().unwrap();
    let escape = &compiled.fragment.machine.transitions[1];
    assert_eq!(escape.sources.as_ref().unwrap(), &["m1_a__work"]);
}
