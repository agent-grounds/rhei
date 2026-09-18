//! Child-owned settings cannot be consumed through concrete generated names.
//! §FS-rhei-library.1.2 §AR-rhei-library.4
use super::*;

fn wrapper(kind: &str) -> Block {
    let mut child = simple("leaf");
    child.local.as_mut().unwrap().settings = serde_json::json!({(kind): {"private": {}}});
    let mut parent = simple("wrapper");
    parent.children.push(("leaf".into(), child));
    parent
}

// Cover each typed settings-reference position independently so an earlier
// denial cannot hide a later unchecked position. §FS-rhei-library.1.2
const USES: &[(&str, &str)] = &[
    ("agents", "agent: REF"),
    ("models", "model: REF"),
    ("models", "all_models: [REF]"),
    ("models", "machine-models"),
    ("models", "task-model"),
    ("mcp_servers", "mcp_servers: [REF]"),
    ("mcp_servers", "mcp_servers: [{id: REF}]"),
    ("skills", "skills: [REF]"),
    ("skills", "skills: [{id: REF}]"),
    ("mcp_servers", "transition-mcp"),
    ("skills", "transition-skill"),
    ("agents", "target"),
    ("models", "target"),
    ("agents", "all-targets"),
    ("models", "all-targets"),
    ("agents", "snapshot-target"),
    ("models", "snapshot-target"),
    ("agents", "task-target"),
    ("models", "task-target"),
    ("agents", "default"),
    ("models", "default"),
    ("mcp_servers", "default"),
    ("skills", "default"),
    ("agents", "model-default-agent"),
    ("agents", "model-agent-binding"),
];

fn reference(parent: &mut Block, kind: &str, position: &str, value: &str) {
    let f = parent.local.as_mut().unwrap();
    let target = if kind == "agents" {
        format!("{value}:provider:external-model")
    } else {
        format!("external-agent:provider:{value}")
    };
    let state = match position {
        "machine-models" => {
            f.machine.models = vec![value.into()];
            return;
        }
        "task-model" => {
            f.tasks[0].tasks[0].model = Some(value.into());
            return;
        }
        "task-target" => {
            f.tasks[0].tasks[0].target = Some(target);
            return;
        }
        "transition-mcp" => {
            f.machine.transitions[0].mcp_unavailable =
                Some(serde_yaml::from_str(&format!("[{value}]")).unwrap());
            return;
        }
        "transition-skill" => {
            f.machine.transitions[0].skill_unavailable =
                Some(serde_yaml::from_str(&format!("[{value}]")).unwrap());
            return;
        }
        "default" => {
            let (field, value) = match kind {
                "agents" => ("agent", serde_json::json!(value)),
                "models" => ("model", serde_json::json!(value)),
                "mcp_servers" => (kind, serde_json::json!([value])),
                "skills" => (kind, serde_json::json!([{"id": value}])),
                _ => unreachable!(),
            };
            f.settings["defaults"] = serde_json::json!({(field): value});
            return;
        }
        "model-default-agent" => {
            f.settings["models"] = serde_json::json!({"local-model": {"default_agent": value}});
            return;
        }
        "model-agent-binding" => {
            f.settings["models"] = serde_json::json!({"local-model": {"agents": {(value): {}}}});
            return;
        }
        "target" => format!("target: {target}"),
        "all-targets" => format!("all_targets: [{target}]"),
        "snapshot-target" => {
            format!("snapshot: {{inherit: {{name: audit, select: {{target: '{target}'}}}}}}")
        }
        snippet => snippet.replace("REF", value),
    };
    f.machine.states.insert("work".into(), serde_yaml::from_str(&state).unwrap());
}

#[test]
fn exposure_denies_generated_settings_in_every_typed_reference_position() {
    for &(kind, position) in USES {
        let mut parent = wrapper(kind);
        reference(&mut parent, kind, position, "m4_leaf__private");
        let error = parent.compile().expect_err(position);
        for fragment in [
            kind,
            "m4_leaf__private",
            "private",
            "expose",
            "none declared",
            "/leaf/template.yaml",
            "/wrapper/template.yaml",
        ] {
            assert!(error.contains(fragment), "{kind} {position}: missing {fragment:?} in {error}");
        }
    }
}

#[test]
fn exposure_preserves_external_settings_in_every_typed_reference_position() {
    for &(kind, position) in USES {
        // Neither ordinary external names nor generated-looking unowned names
        // acquire child ownership from their spelling. §FS-rhei-library.1.2
        for value in ["external", "m4_leaf__unowned"] {
            let mut parent = wrapper(kind);
            reference(&mut parent, kind, position, value);
            let compiled = parent.compile().unwrap_or_else(|e| panic!("{kind} {position}: {e}"));
            let f = &compiled.fragment;
            let rendered = format!(
                "{}\n{}\n{:?}",
                serde_yaml::to_string(&f.machine).unwrap(),
                f.settings,
                f.tasks
            );
            assert!(rendered.contains(value), "lost external {kind} {value} at {position}");
        }
    }
}

#[test]
fn exposure_preserves_local_and_registry_specific_settings_ownership() {
    for (kind, position) in &USES[..2] {
        let other_kind = if *kind == "agents" { "models" } else { "agents" };
        let mut parent = wrapper(other_kind);
        reference(&mut parent, kind, position, "m4_leaf__private");
        // The same spelling belongs to a child in another registry only.
        parent.clone().compile().unwrap();
        parent.local.as_mut().unwrap().settings[*kind] =
            serde_json::json!({"m4_leaf__private": {}});
        let mounted = group(vec![("outer", parent)]).compile().unwrap();
        let expected = "m5_outer__m4_leaf__private";
        assert!(mounted.fragment.settings[*kind].get(expected).is_some());
        let state = &mounted.fragment.machine.states["m5_outer__work"];
        let actual = if *kind == "agents" {
            state.agent.as_ref().map(|a| a.0.as_str())
        } else {
            state.model.as_deref()
        };
        assert_eq!(actual, Some(expected));
    }
    for (kind, position) in [("mcp_servers", "mcp_servers: [REF]"), ("skills", "skills: [REF]")] {
        let mut parent = wrapper(kind);
        parent.local.as_mut().unwrap().settings[kind] = serde_json::json!({"m4_leaf__unowned": {}});
        reference(&mut parent, kind, position, "m4_leaf__unowned");
        parent.compile().unwrap();
    }
}

#[test]
fn exposure_uses_actual_child_ownership_after_nested_compatibility_renaming() {
    let mut inner = group(vec![("leaf", wrapper("agents").children.remove(0).1)]);
    expose(&mut inner, "leaf", "leaf");
    inner.manifest.compatibility.settings.insert("stable".into(), "leaf.private".into());
    let mut parent = simple("parent");
    parent.children.push(("inner".into(), inner));
    reference(&mut parent, "agents", "agent: REF", "m5_inner__stable");
    let error = parent.compile().unwrap_err();
    assert!(error.contains("private generated agents 'm5_inner__stable'"), "{error}");
}

#[test]
fn exposure_requires_public_syntax_even_for_an_exposed_generated_identity() {
    let mut parent = wrapper("agents");
    parent.children[0].1.manifest.expose.settings.agents.insert(
        "reviewer".into(),
        ExposureTarget { local: Some("private".into()), ..Default::default() },
    );
    reference(&mut parent, "agents", "agent: REF", "m4_leaf__reviewer");
    let error = parent.clone().compile().unwrap_err();
    assert!(error.contains("leaf.reviewer"), "{error}");
    reference(&mut parent, "agents", "agent: REF", "leaf.reviewer");
    let compiled = parent.compile().unwrap();
    assert_eq!(
        compiled.fragment.machine.states["work"].agent.as_ref().unwrap().0,
        "m4_leaf__reviewer"
    );
}
