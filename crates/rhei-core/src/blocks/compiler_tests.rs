//! Operation-level regression coverage. §AR-rhei-library.2–5
use super::*;
use crate::ast::{Rhei, Structure};
use std::path::PathBuf;

fn leaf(name: &str, states: &str, tasks: &str) -> Block {
    let machine = crate::state_machine::StateMachine::parse_fragment(states).unwrap();
    let structure = Structure { max_levels: 4, node_kinds: vec!["task".into(), "worker".into()] };
    let tasks = crate::parser::parse_workspace_tasks_with_structure(tasks, &structure).unwrap();
    Block {
        name: name.into(),
        source: PathBuf::from(format!("/{name}/template.yaml")),
        version: "1".into(),
        manifest: BlockManifest {
            ports: Some(ControlPorts {
                entry: "work".into(),
                exits: BTreeMap::from([("done".into(), "done".into())]),
            }),
            ..Default::default()
        },
        local: Some(Fragment {
            machine,
            plan: Rhei {
                title: name.into(),
                states: name.into(),
                states_declared: true,
                structure,
                metadata: None,
                content_sections: vec![],
                tasks: vec![],
            },
            tasks: vec![TaskFile { path: "tasks/01-job.md".into(), tasks }],
            settings: serde_json::json!({}),
            files: BTreeMap::new(),
        }),
        children: vec![],
    }
}
fn simple(name: &str) -> Block {
    leaf(name, "name: local\nversion: 1\nstates:\n  work: {initial: true}\n  done: {final: true}\ntransitions: [{from: work, to: done}]\n", "### Task job: Work\n**State:** work\n\nDo work.\n")
}
fn group(children: Vec<(&str, Block)>) -> Block {
    let mounts =
        children.iter().map(|(a, b)| Mount { alias: (*a).into(), block: b.name.clone() }).collect();
    Block {
        name: "flow".into(),
        source: "/flow/template.yaml".into(),
        version: "1".into(),
        manifest: BlockManifest { mounts, ..Default::default() },
        local: None,
        children: children.into_iter().map(|(a, b)| (a.into(), b)).collect(),
    }
}
fn expose(group: &mut Block, head: &str, tail: &str) {
    group.manifest.ports = Some(ControlPorts {
        entry: format!("{head}.entry"),
        exits: BTreeMap::from([("done".into(), format!("{tail}.done"))]),
    });
}

#[test]
fn recursive_expansion_preserves_every_alias_segment() {
    let mut inner = group(vec![("a", simple("first")), ("b", simple("second"))]);
    expose(&mut inner, "a", "b");
    let mut outer = group(vec![("outer", inner)]);
    outer.source = "/outer/template.yaml".into();
    let compiled = outer.compile().unwrap();
    let m = &compiled.fragment.machine;
    assert!(m.states.contains_key("m5_outer__m1_a__work"));
    assert!(!m.states["m5_outer__m1_a__done"].terminal);
    assert!(m.states["m5_outer__m1_b__done"].terminal);
    assert_eq!(m.profiles.as_ref().unwrap()["flow"].initial, "m5_outer__m1_a__work");
    assert!(m.states.values().all(|s| !s.initial));
    assert_eq!(compiled.origins.len(), 4);
}

#[test]
fn primary_profiles_fold_and_internal_level_rules_stay_owned() {
    let mut a = simple("a");
    let m = &mut a.local.as_mut().unwrap().machine;
    m.profiles = Some(serde_yaml::from_str("primary: {initial: work, allowed: [work, done]}\npanel: {initial: work, allowed: [work, done]}").unwrap());
    m.node_policy = Some(serde_yaml::from_str("root: primary\ndefault: primary\nby_type: {task: panel}\noverrides: [{match: {level: 2}, profile: panel}]").unwrap());
    let b = simple("b");
    // Keep the panel terminal unconsumed by placing that owner at the tail.
    let compiled = group(vec![("b", b), ("a", a)]).compile().unwrap();
    let m = &compiled.fragment.machine;
    assert!(!m.profiles.as_ref().unwrap().contains_key("m1_a__primary"));
    assert_eq!(m.node_policy.as_ref().unwrap().by_type["m1_a__task"], "m1_a__panel");
    assert!(m.node_policy.as_ref().unwrap().overrides.iter().all(|r| r
        .match_
        .node_type
        .as_ref()
        .unwrap()
        .starts_with("m1_a__")));
    assert_eq!(m.profile_for_node("m1_b__task", 2).unwrap().initial, "m1_b__work");
}

#[test]
fn typed_reference_rewriting_preserves_fenced_prose_and_prior_kinds() {
    let mut a = leaf("a", "name: x\nversion: 1\nstates:\n  work: {}\n  done: {final: true, snapshot: {inherit: {name: session, select: {state: work}}}}\ntransitions: [{from: work, to: done}]", "### Worker job: First\n**State:** work\n\n```markdown\n### Task example: Literal\n**State:** work\n```\n\n### Task second: Next\n**State:** work\n**Prior:** Worker job\n");
    a.local.as_mut().unwrap().machine.states.get_mut("work").unwrap().description =
        Some("work remains prose".into());
    let c = group(vec![("a", a)]).compile().unwrap();
    let tasks = &c.fragment.tasks[0].tasks;
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[1].prior_kinds, vec![Some("m1_a__worker".into())]);
    assert!(tasks[0].content.contains("### Task example: Literal"));
    assert!(tasks[0].content.contains("**State:** work"));
    assert_eq!(
        c.fragment.machine.states["m1_a__done"]
            .snapshot
            .as_ref()
            .unwrap()
            .inherit
            .as_ref()
            .unwrap()
            .select
            .as_ref()
            .unwrap()
            .state
            .as_deref(),
        Some("m1_a__work")
    );
}

#[test]
fn file_pass_reaches_every_consumer_without_changing_requiredness() {
    let mut a = simple("a");
    a.local.as_mut().unwrap().machine.states.get_mut("work").unwrap().outputs =
        serde_yaml::from_str("[{name: report, path: runtime/report.md}]").unwrap();
    a.manifest.data.outputs.insert(
        "report".into(),
        DataEndpoint {
            kind: DataKind::StateFile,
            state: Some("work".into()),
            task: None,
            name: "report".into(),
        },
    );
    let mut b = simple("b");
    for state in b.local.as_mut().unwrap().machine.states.values_mut() {
        state.inputs =
            serde_yaml::from_str("[{name: brief, path: runtime/local.md, optional: true}]")
                .unwrap();
    }
    b.manifest.data.inputs.insert(
        "brief".into(),
        DataEndpoint {
            kind: DataKind::StateFile,
            state: Some("work".into()),
            task: None,
            name: "brief".into(),
        },
    );
    let mut g = group(vec![("a", a), ("b", b)]);
    g.manifest.seams = Some(vec![Seam {
        from: "a.done".into(),
        to: "b.entry".into(),
        pass: BTreeMap::from([("a.report".into(), "b.brief".into())]),
    }]);
    let c = g.compile().unwrap();
    for name in ["m1_b__work", "m1_b__done"] {
        let input = &c.fragment.machine.states[name].inputs[0];
        assert_eq!(input.path, "runtime/blocks/m1_a__/runtime/report.md");
        assert!(input.optional);
    }
}

#[test]
fn task_export_uses_exact_producer_and_all_provides() {
    let mut a = leaf("a", "name: x\nversion: 1\nstates: {work: {}, done: {final: true}}\ntransitions: [{from: work, to: done}]", "### Task job: First\n**State:** work\n**Provides:** first, second\n\n### Task other: Other\n**State:** work\n**Provides:** alien\n");
    a.manifest.data.outputs.insert(
        "result".into(),
        DataEndpoint {
            kind: DataKind::TaskExport,
            state: None,
            task: Some("job".into()),
            name: "second".into(),
        },
    );
    let mut b = simple("b");
    b.manifest.data.inputs.insert(
        "brief".into(),
        DataEndpoint {
            kind: DataKind::TaskExport,
            state: None,
            task: Some("job".into()),
            name: "brief".into(),
        },
    );
    let mut g = group(vec![("a", a.clone()), ("b", b)]);
    g.manifest.seams = Some(vec![Seam {
        from: "a.done".into(),
        to: "b.entry".into(),
        pass: BTreeMap::from([("a.result".into(), "b.brief".into())]),
    }]);
    let c = g.compile().unwrap();
    let reference = &c.fragment.tasks[1].tasks[0].consumes[0];
    assert_eq!(reference.task.to_string(), "m1_a__job");
    assert_eq!(reference.name, "m1_a__second");
    a.manifest.data.outputs.get_mut("result").unwrap().name = "alien".into();
    assert!(a.compile().unwrap_err().contains("missing export 'job:alien'"));
}

#[test]
fn compatibility_is_checked_at_root_and_mounted_boundaries() {
    let mut wrapper = group(vec![("a", simple("a"))]);
    expose(&mut wrapper, "a", "a");
    wrapper.manifest.compatibility.states.insert("finish".into(), "a.done".into());
    wrapper.manifest.compatibility.tasks.insert("stable".into(), "a.job".into());
    let root = wrapper.clone().compile().unwrap();
    assert!(root.fragment.machine.states.contains_key("finish"));
    assert_eq!(root.fragment.tasks[0].path, PathBuf::from("tasks/01-job.md"));
    let mut outer = group(vec![("outer", wrapper.clone())]);
    outer.source = "/outer/template.yaml".into();
    let mounted = outer.compile().unwrap();
    assert!(mounted.fragment.machine.states.contains_key("m5_outer__finish"));
    assert_eq!(mounted.fragment.tasks[0].tasks[0].id.to_string(), "m5_outer__stable");
    wrapper.manifest.compatibility.states.insert("ghost".into(), "a.absent".into());
    assert!(wrapper.compile().unwrap_err().contains("unresolved compatibility state"));
}

#[test]
fn local_fragment_and_settings_survive_children() {
    let mut parent = simple("parent");
    parent.children.push(("child".into(), simple("child")));
    parent.manifest.mounts.push(Mount { alias: "child".into(), block: "child".into() });
    parent.local.as_mut().unwrap().settings =
        serde_json::json!({"agents": {"worker": {"command": ["local"]}}});
    parent.children[0].1.local.as_mut().unwrap().settings = serde_json::json!({"agents": {"worker": {"command": ["child"]}}, "models": {"smart": {"default_agent": "worker"}}});
    let c = group(vec![("outer", parent)]).compile().unwrap();
    assert!(c.fragment.machine.states.contains_key("m5_outer__work"));
    assert!(c.fragment.machine.states.contains_key("m5_outer__m5_child__work"));
    assert_eq!(c.fragment.tasks.len(), 2);
    assert_eq!(
        c.fragment.settings["models"]["m5_outer__m5_child__smart"]["default_agent"],
        "m5_outer__m5_child__worker"
    );
    assert!(c.fragment.settings["agents"].get("m5_outer__worker").is_some());
}

#[test]
fn missing_ports_nonterminal_exits_and_seam_guards_fail_without_panics() {
    let mut a = simple("a");
    a.manifest.ports = None;
    assert!(group(vec![("a", a)]).compile().unwrap_err().contains("missing ports"));
    let mut a = simple("a");
    a.manifest.ports.as_mut().unwrap().exits.insert("done".into(), "work".into());
    assert!(a.compile().unwrap_err().contains("must be terminal"));
    for field in ["condition", "gate", "callback", "expression"] {
        let result =
            serde_yaml::from_str::<Seam>(&format!("from: a.done\nto: b.entry\n{field}: false"));
        assert!(result.unwrap_err().to_string().contains("unknown field"));
    }
}

#[path = "reference_tests.rs"]
mod reference_tests;
