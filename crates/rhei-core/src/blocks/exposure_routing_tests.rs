//! Overlapping primary lanes share references, never definitions. §FS-rhei-library.4
use super::*;

fn observing_wrapper() -> Block {
    let mut child = simple("child");
    child.manifest.expose = serde_yaml::from_str("states: {ready: {local: work}}").unwrap();
    let machine = &mut child.local.as_mut().unwrap().machine;
    machine.profiles =
        Some(serde_yaml::from_str("primary: {initial: work, allowed: [work, done]}").unwrap());
    machine.node_policy = Some(serde_yaml::from_str("root: primary\ndefault: primary").unwrap());
    let mut wrapper = simple("wrapper");
    let machine = &mut wrapper.local.as_mut().unwrap().machine;
    machine.profiles = Some(
        serde_yaml::from_str(
            "observer: {initial: child.ready, allowed: [done, child.ready, work]}",
        )
        .unwrap(),
    );
    machine.node_policy = Some(serde_yaml::from_str("root: observer\ndefault: observer").unwrap());
    wrapper.children.push(("child".into(), child));
    wrapper.manifest.mounts.push(Mount { alias: "child".into(), block: "child".into() });
    expose(&mut wrapper, "child", "child");
    wrapper
}

#[test]
fn exposure_primary_lanes_preserve_first_occurrence_and_distinct_states() {
    for nested in [false, true] {
        let mut wrapper = observing_wrapper();
        let (block, prefix, public) = if nested {
            wrapper.manifest.expose =
                serde_yaml::from_str("states: {approved: {mount: child, name: ready}}").unwrap();
            (group(vec![("review", wrapper)]), "m6_review__", "approved")
        } else {
            (wrapper, "", "m5_child__ready")
        };
        let compiled = block.compile().unwrap();
        let expected: Vec<_> = ["done", public, "work", "m5_child__done"]
            .into_iter()
            .map(|state| format!("{prefix}{state}"))
            .collect();
        assert_eq!(compiled.primary, expected);
        let machine = &compiled.fragment.machine;
        let flow = &machine.profiles.as_ref().unwrap()["flow"];
        assert_eq!(flow.allowed, expected);
        assert_eq!(flow.initial, format!("{prefix}{public}"));
        assert_eq!(machine.states.len(), expected.len());
        assert!(expected.iter().all(|state| machine.states.contains_key(state)));
        assert_eq!(machine.transitions.len(), 2);
        assert!(machine.transitions.iter().any(|edge| {
            edge.from.0 == format!("{prefix}work") && edge.to.0 == format!("{prefix}done")
        }));
        assert!(machine.transitions.iter().any(|edge| {
            edge.from.0 == flow.initial && edge.to.0 == format!("{prefix}m5_child__done")
        }));
        let yaml = serde_yaml::to_string(machine).unwrap();
        crate::state_machine::StateMachine::from_yaml_str(&yaml).unwrap();
    }
}

#[test]
fn exposure_lane_overlap_does_not_hide_authored_profile_duplicates() {
    for owner in ["wrapper", "child", "internal"] {
        let mut wrapper = observing_wrapper();
        let (block, profile, state) = match owner {
            "child" => (&mut wrapper.children[0].1, "primary", "work"),
            "internal" => {
                let profiles = wrapper.local.as_mut().unwrap().machine.profiles.as_mut().unwrap();
                profiles.insert("internal".into(), profiles["observer"].clone());
                (&mut wrapper, "internal", "child.ready")
            }
            _ => (&mut wrapper, "observer", "child.ready"),
        };
        block
            .local
            .as_mut()
            .unwrap()
            .machine
            .profiles
            .as_mut()
            .unwrap()
            .get_mut(profile)
            .unwrap()
            .allowed
            .push(state.into());
        let machine = wrapper.compile().unwrap().fragment.machine;
        let yaml = serde_yaml::to_string(&machine).unwrap();
        let error =
            crate::state_machine::StateMachine::from_yaml_str(&yaml).unwrap_err().to_string();
        assert!(error.contains("duplicate 'allowed' entry 'm5_child__ready'"), "{owner}: {error}");
    }
}
