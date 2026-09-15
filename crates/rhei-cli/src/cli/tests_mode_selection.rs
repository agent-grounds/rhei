    fn program_poll_machine(required_input: bool) -> rhei_validator::StateMachine {
        let input = if required_input {
            "    inputs:\n      - name: approval\n        path: runtime/approval.md\n"
        } else {
            ""
        };
        rhei_validator::StateMachine::from_yaml_str(&format!(
            r#"name: mode-probe
version: 1
states:
  polling:
    initial: true
    description: Poll a program
    program: ignored
    poll:
      interval: 5m
      max_attempts: 3
{input}  done:
    description: Done
    final: true
transitions:
  - from: polling
    to: done
    condition: pollAttempts >= pollMaxAttempts
  - from: polling
    to: done
    exit_code: 0
  - from: polling
    to: polling
    exit_code: 75
"#
        ))
        .expect("valid program poll machine")
    }

    fn mode_probe(
        rhei: &rhei_core::ast::Rhei,
        machine: rhei_validator::StateMachine,
        opts: &RunOptions,
        root: &Path,
    ) -> bool {
        should_use_agent_mode(
            rhei,
            &rhei_validator::MachineSet::single(machine),
            &default_settings(),
            opts,
            &ReadySetRoots::plan_only(root),
        )
        .expect("mode selection should succeed")
    }

    /// A future retry deadline governs when a program poll runs, not whether
    /// the run keeps the subprocess engine that can execute it. All other
    /// readiness, scope, selection, and spawn-suppression rules still apply.
    /// §FS-rhei-run.3
    #[test]
    fn mode_selection_keeps_an_eligible_future_program_poll_reachable() {
        let deadline = current_unix_secs() + 86_400;
        let future = rhei_core::parse(&format!(
            r#"# Rhei: Future program poll

---
metadata:
  tasks:
    1:
      pollNextAttemptAt:
        polling: {deadline}
---

## Tasks

### Task 1: Poll
**State:** polling
"#
        ))
        .expect("valid future poll plan");
        let missing_input = rhei_core::parse(
            r#"# Rhei: Ineligible program poll

## Tasks

### Task 1: Poll
**State:** polling
"#,
        )
        .expect("valid ineligible poll plan");
        let dir = tempfile::tempdir().expect("temporary artifact root");
        let machine = program_poll_machine(false);

        assert!(
            find_runnable_tasks(
                &future,
                &rhei_validator::MachineSet::single(machine.clone()),
                &ReadySetRoots::plan_only(dir.path()),
                &HashSet::new(),
            )
            .is_empty(),
            "the ordinary ready set must continue to enforce poll backoff"
        );

        let default_opts = default_run_options();
        let mut no_agent = default_run_options();
        no_agent.agent.no_agent = true;
        let mut no_program = default_run_options();
        no_program.program.no_program = true;
        let mut out_of_scope = default_run_options();
        out_of_scope.standalone.rhei = vec!["elsewhere".to_string()];

        let observed = vec![
            (
                "future program poll with spawning enabled",
                mode_probe(&future, machine.clone(), &default_opts, dir.path()),
            ),
            (
                "future program poll under --no-agent",
                mode_probe(&future, machine.clone(), &no_agent, dir.path()),
            ),
            (
                "future program poll under --no-program",
                mode_probe(&future, machine.clone(), &no_program, dir.path()),
            ),
            (
                "future program poll outside --rhei scope",
                mode_probe(&future, machine.clone(), &out_of_scope, dir.path()),
            ),
            (
                "program poll missing a required input",
                mode_probe(
                    &missing_input,
                    program_poll_machine(true),
                    &default_opts,
                    dir.path(),
                ),
            ),
        ];

        assert_eq!(
            observed,
            vec![
                ("future program poll with spawning enabled", true),
                ("future program poll under --no-agent", true),
                ("future program poll under --no-program", false),
                ("future program poll outside --rhei scope", false),
                ("program poll missing a required input", false),
            ]
        );
    }
