// Member plan paths and machine callback bases are independent routing facts.
// §FS-rhei-panta.6.2 §AR-rhei-panta.5
mod member_execution_context_tests {
    use super::super::*;

    fn context(input: &Path) -> ExecutionMachines {
        let loaded = load_plan(input).unwrap();
        let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, None).unwrap();
        ExecutionMachines::build(&resolved, input, &loaded).unwrap()
    }

    fn canonical(path: &Path) -> PathBuf {
        rhei_core::platform::canonical_path(path).unwrap()
    }

    #[test]
    fn issue_205_member_plan_paths_preserve_inherited_and_declared_callback_bases() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        fs::write(project.join("index.panta.md"), "# Panta: Routing\n**States:** inherited\n")
            .unwrap();
        let machine = "name: inherited\nversion: 1\nstates:\n  ready:\n    initial: true\n  done:\n    final: true\ntransitions:\n  - from: ready\n    to: done\n";
        fs::write(project.join("states.yaml"), machine).unwrap();
        for (name, declaration) in [("inherited", ""), ("own", "**States:** own\n")] {
            let member = project.join(name);
            fs::create_dir_all(member.join("tasks")).unwrap();
            fs::write(member.join("index.rhei.md"), format!("# Rhei: Member\n{declaration}"))
                .unwrap();
            fs::write(member.join("tasks/01-work.md"), "### Task 1: Work\n**State:** ready\n")
                .unwrap();
        }
        fs::write(project.join("own/states.yaml"), machine.replace("inherited", "own")).unwrap();
        fs::write(
            project.join("single.rhei.md"),
            "# Rhei: Single\n\n## Tasks\n\n### Task 1: Work\n**State:** ready\n",
        )
        .unwrap();

        let machines = context(project);
        for name in ["inherited", "own"] {
            let paths = machines.callbacks_for_str(&format!("{name}.1"));
            assert_eq!(paths.plan_path, canonical(&project.join(name)));
            let machine_root =
                if name == "own" { project.join("own") } else { project.to_path_buf() };
            assert_eq!(paths.working_dir, canonical(&machine_root));
            assert_eq!(
                paths.state_machine_path,
                Some(canonical(&machine_root.join("states.yaml")))
            );
        }
        let single = machines.callbacks_for_str("single.1");
        assert_eq!(single.plan_path, canonical(&project.join("single.rhei.md")));
        assert_eq!(single.working_dir, canonical(project));
        assert_eq!(machines.default_callbacks.plan_path, canonical(project));
    }

    #[test]
    fn issue_205_bare_plan_keeps_the_invocation_plan_path() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("single.rhei.md");
        fs::write(&input, "# Rhei: Single\n\n## Tasks\n\n### Task 1: Work\n**State:** draft\n")
            .unwrap();
        let machines = context(&input);
        assert_eq!(machines.callbacks_for_str("single.1").plan_path, canonical(&input));
        assert_eq!(machines.callbacks_for_str("single.1").working_dir, canonical(dir.path()));
    }
}
