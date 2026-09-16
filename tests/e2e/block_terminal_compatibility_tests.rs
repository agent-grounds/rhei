//! Public terminal routes and human approval survive real wrapper composition.
//! §FS-rhei-library.7.1–2 §FS-rhei-library.5
use super::block_composition_support::run_compose;
use super::*;
use std::fs;

#[test]
fn block_terminal_compatibility_keeps_root_and_mounted_routes_and_human_cancel() {
    let root = unique_temp_dir("block-terminal-compatibility");
    for mounted in [false, true] {
        for cancel in [false, true] {
            let name = format!("out-{mounted}-{cancel}");
            let output = root.join(&name);
            let mut args = if mounted {
                vec![
                    "instantiate",
                    "--mount",
                    "outer=changeset-review",
                    "--set",
                    "outer.change_ref=HEAD~3",
                ]
            } else {
                vec!["instantiate", "changeset-review", "HEAD~3"]
            };
            args.extend(["--output", output.to_str().unwrap()]);
            assert_success(&run_compose(&root, &args));
            let machine =
                rhei_core::state_machine::StateMachine::from_yaml_file(output.join("states.yaml"))
                    .unwrap();
            let prefix = if mounted { "m5_outer__" } else { "" };
            let state = |name: &str| format!("{prefix}{name}");
            assert_eq!(machine.states.values().filter(|s| s.terminal).count(), 2);
            for from in ["split", "review", "final-fix"] {
                assert!(machine
                    .transitions
                    .iter()
                    .any(|r| r.from.0 == state(from) && r.to.0 == state("completed")));
            }
            assert!(machine.is_cancellation(&state("cancelled")));
            let gate = state("human-review");
            let escape = machine
                .transitions
                .iter()
                .find(|r| {
                    r.from.0 == "*"
                        && r.to.0 == state("cancelled")
                        && machine.transition_matches_source(r, &gate)
                })
                .unwrap();
            assert!(
                !machine.transition_matches_source(escape, &state("final-fix")),
                "review escape cannot capture fix work"
            );
            let task_path = output.join(if mounted {
                "tasks/m5_outer__/01-coordinate.md"
            } else {
                "tasks/01-coordinate.md"
            });
            let task = fs::read_to_string(&task_path)
                .unwrap()
                .replace(&format!("**State:** {}", state("split")), &format!("**State:** {gate}"));
            fs::write(&task_path, task).unwrap();
            let task_id = format!("{name}.{prefix}coordinate");
            if !cancel {
                let decision = machine.states[&state("final-fix")]
                    .inputs
                    .iter()
                    .find(|a| a.name == "final-decision")
                    .unwrap()
                    .path
                    .replace("{task_id}", &task_id);
                let path = output.join(decision);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, "Approved decision\n").unwrap();
            }
            let to = state(if cancel { "cancelled" } else { "final-fix" });
            assert_success(&run_compose(
                &root,
                &[
                    "transition",
                    output.to_str().unwrap(),
                    "--task",
                    &format!("{prefix}coordinate"),
                    "--from",
                    &gate,
                    "--to",
                    &to,
                    "--result",
                    "Human decision",
                    "--no-callbacks",
                ],
            ));
            assert!(fs::read_to_string(task_path).unwrap().contains(&format!("**State:** {to}")));
        }
    }
}
