// Human-facing first-visit notes at the decision sink, rather than tests of
// the eligibility predicate. No subprocess or provider is contacted here.
// §FS-rhei-agent-visit-pairing.6

mod first_visit_reuse {
    use super::super::*;

    #[derive(Default)]
    struct Messages(Mutex<Vec<String>>);

    impl rhei_tui::EventSink for Messages {
        fn emit(&self, event: rhei_tui::RunEvent) {
            if let rhei_tui::RunEvent::Message { text, .. } = event {
                self.0.lock().unwrap().push(text);
            }
        }
    }

    #[test]
    fn first_visit_reuse_announces_only_actual_skips_once_per_visit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let runtime = root.join("runtime");
        fs::create_dir_all(&runtime).unwrap();
        fs::write(runtime.join("digest-alpha.md"), "seeded for alpha\n").unwrap();
        let settings: RheiSettings = serde_json::from_str(
            r#"{"agents":{"mock":{"command":["unused"]}},
                "models":{"alpha":{"provider":"mock","model":"alpha"},
                          "beta":{"provider":"mock","model":"beta"}}}"#,
        )
        .unwrap();
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"name: first-visit-note
version: 1
states:
  work:
    initial: true
    all_targets: ["mock:mock:alpha", "mock:mock:beta"]
    outputs:
      - { name: digest, path: "runtime/digest-{model}.md" }
  review: { gating: true }
  cancelled: { final: true }
transitions:
  - { from: work, to: review }
  - { from: "*", to: cancelled }
"#,
        )
        .unwrap();
        let mut rhei =
            rhei_core::parse("# Rhei: Notes\n\n## Tasks\n\n### Task 1: Work\n**State:** work\n")
                .unwrap();
        rhei.tasks[0].id = parse_task_id("plan.1");
        let task = &rhei.tasks[0];
        let state = machine.states.get("work").unwrap();
        let opts = default_run_options();
        let invocations =
            resolve_agent_invocations_for_task(&machine, "work", &settings, &opts, Some(task)).unwrap();
        let messages = Arc::new(Messages::default());
        let sink: Arc<dyn rhei_tui::EventSink> = messages.clone();
        let reset = ResetOffer::new(&root.join("plan.rhei.md"));
        let decide = |notes| {
            agent_invocations_to_spawn(
                &rhei, root, &runtime, task, &machine, "work", state, invocations.clone(), notes,
            )
        };

        // Predicate-only callers can query repeatedly during provider waits.
        for _ in 0..3 {
            assert_eq!(decide(None).len(), 1);
            assert!(task_has_pending_agent_invocations(
                root, &runtime, task, "work", "work", &machine, rhei.metadata.as_ref(),
                state, &settings, &opts, Some("review"),
            )
            .unwrap());
        }
        assert!(messages.0.lock().unwrap().is_empty(), "readiness queries stay silent");
        for _ in 0..3 {
            let pending = decide(Some((&sink, &reset)));
            assert_eq!(pending.len(), 1, "beta still owes its own output");
            assert_eq!(pending[0].model.as_deref(), Some("beta"));
        }
        let said = messages.0.lock().unwrap();
        assert_eq!(said.len(), 1, "an actual first-visit skip must be announced exactly once: {said:?}");
        let note = &said[0];
        assert!(note.contains("task plan.1 state 'work': reusing first-visit outputs"), "{note}");
        // Authored artifact paths can retain forward slashes on Windows.
        let expected_path = runtime.join("digest-alpha.md").display().to_string().replace('\\', "/");
        assert!(note.replace('\\', "/").contains(&expected_path), "{note}");
        assert!(!note.contains("digest-beta.md"), "beta will spawn: {note}");
        assert!(!note.contains("{model}"), "the note names resolved paths: {note}");
        assert!(note.contains("no agent spawned"), "{note}");
    }
}
