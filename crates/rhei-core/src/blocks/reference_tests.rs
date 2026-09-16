//! Counted states and registry-specific ownership. §FS-rhei-library.4
use super::*;

#[test]
fn counted_states_keep_visits_at_identity_mounted_and_compatibility_boundaries() {
    let block = leaf(
        "counted",
        "name: counted\nversion: 1\nstates: {work: {visits: 3}, done: {final: true}}\ntransitions: [{from: work, to: done}]",
        "### Task job: Work\n**State:** work-2\n",
    );
    let legacy = block.clone().compile().unwrap();
    assert_eq!(legacy.fragment.tasks[0].tasks[0].state, "work-2");
    let mut wrapper = group(vec![("a", block)]);
    let mounted = wrapper.clone().compile().unwrap();
    assert_eq!(mounted.fragment.tasks[0].tasks[0].state, "m1_a__work-2");
    wrapper.manifest.compatibility.states.insert("stable".into(), "a.work".into());
    let stable = wrapper.compile().unwrap();
    assert_eq!(stable.fragment.tasks[0].tasks[0].state, "stable-2");
    for compiled in [legacy, mounted, stable] {
        let parsed = crate::state_machine::parse_task_state(
            &compiled.fragment.tasks[0].tasks[0].state,
            &compiled.fragment.machine,
        );
        assert_eq!(parsed.visit, Some(2));
        assert!(compiled.fragment.machine.states.contains_key(&parsed.state));
    }
}

#[test]
fn an_exact_numeric_state_name_takes_precedence_over_a_visit_suffix() {
    let block = leaf(
        "exact",
        "name: exact\nversion: 1\nstates: {work: {visits: 3}, work-2: {}, done: {final: true}}\ntransitions: [{from: work, to: work-2}, {from: work-2, to: done}]",
        "### Task job: Work\n**State:** work-2\n",
    );
    let mut wrapper = group(vec![("a", block)]);
    wrapper.manifest.compatibility.states.insert("counted".into(), "a.work".into());
    wrapper.manifest.compatibility.states.insert("exact".into(), "a.work-2".into());
    assert_eq!(wrapper.compile().unwrap().fragment.tasks[0].tasks[0].state, "exact");
}

fn agent_with_external_model() -> Block {
    let mut block = simple("agent");
    let fragment = block.local.as_mut().unwrap();
    fragment.settings = serde_json::json!({"agents": {"local": {"command": ["agent"]}}});
    fragment.machine.states.get_mut("work").unwrap().target = Some("local:provider:local".into());
    fragment.tasks[0].tasks[0].target = Some("local:provider:local".into());
    block
}

#[test]
fn owning_an_agent_does_not_capture_an_external_model_with_the_same_name() {
    let block = agent_with_external_model();
    let legacy = block.clone().compile().unwrap();
    assert_eq!(
        legacy.fragment.machine.states["work"].target.as_deref(),
        Some("local:provider:local")
    );
    let mut wrapper = group(vec![("a", block)]);
    expose(&mut wrapper, "a", "a");
    let mounted = wrapper.clone().compile().unwrap();
    assert_eq!(
        mounted.fragment.machine.states["m1_a__work"].target.as_deref(),
        Some("m1_a__local:provider:local")
    );
    assert_eq!(
        mounted.fragment.tasks[0].tasks[0].target.as_deref(),
        Some("m1_a__local:provider:local")
    );
    wrapper.manifest.compatibility.settings.insert("public".into(), "a.local".into());
    let stable = wrapper.clone().compile().unwrap();
    assert_eq!(
        stable.fragment.machine.states["m1_a__work"].target.as_deref(),
        Some("public:provider:local")
    );
    let mut outer = group(vec![("outer", wrapper)]);
    outer.source = "/outer/template.yaml".into();
    assert_eq!(
        outer.compile().unwrap().fragment.machine.states["m5_outer__m1_a__work"].target.as_deref(),
        Some("m5_outer__public:provider:local")
    );
}

#[test]
fn each_registry_rewrites_only_references_of_its_own_kind() {
    for owned_kind in ["agents", "models", "mcp_servers", "skills"] {
        let mut block = simple("kinds");
        let f = block.local.as_mut().unwrap();
        f.settings = serde_json::json!({
            (owned_kind): {"local": {}},
            "defaults": {"agent": "local", "model": "local", "mcp_servers": ["local"], "skills": [{"id": "local"}]}
        });
        f.machine.models = vec!["local".into()];
        f.machine.states.insert("work".into(), serde_yaml::from_str(
            "agent: local\nmodel: local\nall_models: [local]\ntarget: local:provider:local\nall_targets: [local:provider:local]\nmcp_servers: [local]\nskills: [local]\nsnapshot: {inherit: {name: session, select: {target: 'local:provider:local'}}}"
        ).unwrap());
        f.machine.transitions[0].mcp_unavailable = Some(serde_yaml::from_str("[local]").unwrap());
        f.machine.transitions[0].skill_unavailable = Some(serde_yaml::from_str("[local]").unwrap());
        f.tasks[0].tasks[0].model = Some("local".into());
        let compiled = group(vec![("a", block)]).compile().unwrap();
        let expected = |kind| if owned_kind == kind { "m1_a__local" } else { "local" };
        let state = &compiled.fragment.machine.states["m1_a__work"];
        assert_eq!(state.agent.as_ref().unwrap().0, expected("agents"));
        assert_eq!(state.model.as_deref(), Some(expected("models")));
        assert_eq!(state.all_models, vec![expected("models")]);
        let target = format!("{}:provider:{}", expected("agents"), expected("models"));
        assert_eq!(state.target.as_deref(), Some(target.as_str()));
        assert_eq!(state.all_targets, vec![target.clone()]);
        assert_eq!(
            state
                .snapshot
                .as_ref()
                .unwrap()
                .inherit
                .as_ref()
                .unwrap()
                .select
                .as_ref()
                .unwrap()
                .target
                .as_ref(),
            Some(&target)
        );
        let yaml = serde_yaml::to_value(state).unwrap();
        assert_eq!(yaml["mcp_servers"][0].as_str(), Some(expected("mcp_servers")));
        assert_eq!(yaml["skills"][0].as_str(), Some(expected("skills")));
        let edge = &compiled.fragment.machine.transitions[0];
        assert_eq!(
            edge.mcp_unavailable.as_ref().unwrap()[0].as_str(),
            Some(expected("mcp_servers"))
        );
        assert_eq!(edge.skill_unavailable.as_ref().unwrap()[0].as_str(), Some(expected("skills")));
        assert_eq!(compiled.fragment.machine.models, vec![expected("models")]);
        assert_eq!(compiled.fragment.tasks[0].tasks[0].model.as_deref(), Some(expected("models")));
        let defaults = &compiled.fragment.settings["defaults"];
        assert_eq!(defaults["agent"], expected("agents"));
        assert_eq!(defaults["model"], expected("models"));
        assert_eq!(defaults["mcp_servers"][0], expected("mcp_servers"));
        assert_eq!(defaults["skills"][0]["id"], expected("skills"));
    }
}

#[test]
fn compatibility_collisions_are_checked_within_the_owned_registry() {
    let mut wrapper = simple("wrapper");
    wrapper.local.as_mut().unwrap().settings = serde_json::json!({"models": {"public": {"default_agent": "local", "agents": {"local": {}}}}});
    wrapper.children.push(("a".into(), agent_with_external_model()));
    wrapper.manifest.compatibility.settings.insert("public".into(), "a.local".into());
    let compiled = wrapper.clone().compile().unwrap();
    assert!(compiled.fragment.settings["agents"].get("public").is_some());
    assert_eq!(compiled.fragment.settings["models"]["public"]["default_agent"], "local");
    assert!(compiled.fragment.settings["models"]["public"]["agents"].get("local").is_some());
    wrapper.local.as_mut().unwrap().settings["agents"] = serde_json::json!({"public": {}});
    assert!(wrapper.compile().unwrap_err().contains("compatibility agents 'public' collides"));
}
