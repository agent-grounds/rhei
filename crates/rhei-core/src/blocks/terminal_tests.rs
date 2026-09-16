//! Terminal coalescing compares behavior and rewrites typed references.
//! §FS-rhei-library.7.1
use super::*;

fn terminal_leaf(name: &str) -> Block {
    let mut block = leaf(
        name,
        r#"name: terminals
version: 1
states:
  work: {snapshot: {inherit: {name: session, select: {state: done}}}}
  gate: {final: true, gating: true}
  done: {final: true, instructions: 'Finished {task_id}.'}
  cancelled: {final: true}
transitions:
  - {from: work, to: done}
  - {from: '*', to: cancelled}
"#,
        "### Task job: Finished\n**State:** done\n",
    );
    block.manifest.ports.as_mut().unwrap().exits.insert("done".into(), "gate".into());
    block
}

fn equivalent_wrapper() -> Block {
    let mut wrapper = group(vec![("a", terminal_leaf("a")), ("b", terminal_leaf("b"))]);
    expose(&mut wrapper, "a", "b");
    wrapper.manifest.compatibility.terminals = BTreeMap::from([
        ("completed".into(), vec!["a.done".into(), "b.done".into()]),
        ("cancelled".into(), vec!["a.cancelled".into(), "b.cancelled".into()]),
    ]);
    wrapper.manifest.compatibility.states.insert("start".into(), "a.work".into());
    wrapper
}

#[test]
fn terminal_equivalence_preserves_root_mounted_and_one_target_references() {
    let wrapper = equivalent_wrapper();
    for mounted in [false, true] {
        let block = if mounted {
            let mut outer = group(vec![("outer", wrapper.clone())]);
            outer.source = "/outer/template.yaml".into();
            outer
        } else {
            wrapper.clone()
        };
        let compiled = block.compile().unwrap();
        let machine = &compiled.fragment.machine;
        let prefix = if mounted { "m5_outer__" } else { "" };
        let completed = format!("{prefix}completed");
        let cancelled = format!("{prefix}cancelled");
        assert!(machine.states.contains_key(&completed));
        assert!(machine.is_cancellation(&cancelled));
        assert!(!machine.states.contains_key(&format!("{prefix}m1_a__done")));
        assert!(!machine.states.contains_key(&format!("{prefix}m1_b__cancelled")));
        for file in &compiled.fragment.tasks {
            assert_eq!(file.tasks[0].state, completed);
        }
        for name in [format!("{prefix}start"), format!("{prefix}m1_b__work")] {
            let snapshot = machine.states[&name]
                .snapshot
                .as_ref()
                .unwrap()
                .inherit
                .as_ref()
                .unwrap()
                .select
                .as_ref()
                .unwrap();
            assert_eq!(snapshot.state.as_ref(), Some(&completed));
            assert!(machine.transitions.iter().any(|r| r.from.0 == name && r.to.0 == completed));
        }
        for rule in machine.transitions.iter().filter(|r| r.from.0 == "*") {
            assert_eq!(rule.to.0, cancelled);
            assert_eq!(
                rule.sources.as_ref().unwrap().iter().filter(|s| *s == &cancelled).count(),
                1
            );
        }
        for profile in machine.profiles.as_ref().unwrap().values() {
            assert_eq!(profile.allowed.iter().filter(|s| *s == &completed).count(), 1);
        }
    }
}

#[test]
fn different_prompt_files_coalesce_only_when_the_bound_text_matches() {
    let mut wrapper = equivalent_wrapper();
    for (alias, block) in &mut wrapper.children {
        let fragment = block.local.as_mut().unwrap();
        let state = fragment.machine.states.get_mut("done").unwrap();
        state.instructions = None;
        state.prompt_template = Some(
            serde_yaml::from_str(&format!("name: {alias}-prompt\nvalues: {{who: '{{task_id}}'}}"))
                .unwrap(),
        );
        fragment.machine.prompt_templates.insert(
            format!("{alias}-prompt"),
            crate::state_machine::PromptTemplateDef {
                instructions: "Finished {who}.".into(),
                source: None,
            },
        );
    }
    assert!(wrapper.clone().compile().is_ok());
    wrapper.children[1]
        .1
        .local
        .as_mut()
        .unwrap()
        .machine
        .prompt_templates
        .get_mut("b-prompt")
        .unwrap()
        .instructions = "Different {who}.".into();
    let error = wrapper.clone().compile().unwrap_err();
    assert!(
        error.contains("instructions")
            && error.contains("/a/template.yaml")
            && error.contains("/b/template.yaml"),
        "{error}"
    );
    wrapper.children[1]
        .1
        .local
        .as_mut()
        .unwrap()
        .machine
        .prompt_templates
        .get_mut("b-prompt")
        .unwrap()
        .instructions = "Unbound {other}.".into();
    assert!(wrapper.clone().compile().unwrap_err().contains("does not supply"));
    wrapper.children[1].1.local.as_mut().unwrap().machine.prompt_templates.clear();
    assert!(wrapper.compile().unwrap_err().contains("prompt"));
}

#[test]
fn terminal_equivalence_refuses_roles_and_every_operative_contract_difference() {
    for (field, value) in [
        ("role", "cancellation"),
        ("instructions", "Different work"),
        ("personality", "Different persona"),
        ("gating", "true"),
        ("target", "external:provider:model"),
        ("program", "echo changed"),
        ("inputs", "[{name: report, path: report.md}]"),
        ("outputs", "[{name: report, path: report.md}]"),
    ] {
        let mut wrapper = equivalent_wrapper();
        let states = &mut wrapper.children[1].1.local.as_mut().unwrap().machine.states;
        let mut state = serde_yaml::to_value(&states["done"]).unwrap();
        state[field] = serde_yaml::from_str(value).unwrap();
        states.insert("done".into(), serde_yaml::from_value(state).unwrap());
        let error = wrapper.compile().unwrap_err();
        assert!(error.contains(field), "{field}: {error}");
    }
    let mut wrapper = equivalent_wrapper();
    wrapper.children[1].1.local.as_mut().unwrap().machine.transitions[0].on_enter =
        Some(crate::ast::CallbackRef("cli:changed".into()));
    assert!(wrapper.compile().unwrap_err().contains("transition_contracts"));
    let mut wrapper = equivalent_wrapper();
    wrapper
        .manifest
        .compatibility
        .terminals
        .insert("bad".into(), vec!["a.gate".into(), "b.gate".into()]);
    assert!(wrapper.compile().unwrap_err().contains("unconsumed terminal"));
    let mut wrapper = equivalent_wrapper();
    wrapper.manifest.compatibility.states.insert("other".into(), "a.done".into());
    assert!(wrapper.compile().unwrap_err().contains("claimed more than once"));
}
