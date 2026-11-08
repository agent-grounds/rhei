// Real halt renderer, transition preflight, and manual command construction.
// §FS-rhei-retry.3 §FS-rhei-transition-cmd.2 §FS-rhei-supervision.3.4

mod retry_supervision {
    use super::*;

    #[test]
    fn supervising_halt_preserves_ending_and_names_hold_count_and_retry() {
        for count in [0, 1, 3] {
            let line = render_spent_visit_halt(
                "plan.1",
                "supervising",
                4,
                "brief missing",
                HaltRecovery::Visit { configured: 2, checkpoints: Some(count) },
            );
            assert!(line.contains("The ticket stays in 'supervising'. Its subtree is still held"));
            assert!(line.contains(&format!(
                "{count} unread checkpoint{}.",
                if count == 1 { "" } else { "s" }
            )));
            assert!(line.contains("4 attempts spent"));
            assert!(line.contains(
                "To give this visit 2 more attempts and keep the hold, run: rhei retry plan.1"
            ));
            assert!(line.contains("self-loop would release the subtree"));
        }
        let poll =
            render_spent_visit_halt("plan.1", "waiting", 2, "edit reverted", HaltRecovery::Poll);
        assert!(poll.ends_with("The ticket stays in 'waiting'."));
        assert!(!poll.contains("rhei retry"));
    }

    #[test]
    fn release_flag_rejects_non_supervising_edge_non_self_loop_and_force() {
        let machine = supervision_machine();
        for (force, from, to) in [
            (false, "review", "completed"),
            (false, "supervising", "completed"),
            (true, "supervising", "supervising"),
        ] {
            assert!(release_flag_preflight(true, force, &machine, from, to)
                .unwrap_err()
                .to_string()
                .contains("--release-subtree"));
        }
        assert!(release_flag_preflight(true, false, &machine, "supervising", "supervising").is_ok());
    }

    #[test]
    fn manual_supervisor_release_command_contains_release_subtree() {
        let command = manual_supervisor_release_command(
            Path::new("plan.rhei.md"),
            Some(Path::new("states.yaml")),
            "plan.1",
            "supervising",
        );
        assert_eq!(command, "rhei transition plan.rhei.md --task plan.1 --from supervising --to supervising --release-subtree --state-machine states.yaml");
    }

    // The refusal's message names the target its help does, as typed. §FS-rhei-errors.1.2.1
    #[test]
    fn manual_release_refusal_message_names_the_typed_target() {
        let machine = supervision_machine();
        let mut plan = supervised_plan(&["completed"]);
        let key = parse_task_id("1");
        let pending = checkpoint("1.1", "review", "completed", 1);
        plan.metadata = Some(record_supervision_hold(plan.metadata.as_ref(), &key, Some(&pending)));
        let _scope = enter_help_target(HelpTarget::naming(Path::new("project/auth.rhei.md")));
        let error = manual_release_preflight(
            &machine,
            plan.metadata.as_ref(),
            &key,
            "plan.1",
            "supervising",
            "supervising",
            false,
        )
        .unwrap_err()
        .to_string();
        for expected in [
            "more attempts with the subtree still held: rhei retry project/auth.rhei.md --task plan.1.",
            "To release anyway: rhei transition project/auth.rhei.md --task plan.1 --from supervising \
             --to supervising --release-subtree",
        ] {
            assert!(error.contains(expected), "{error}");
        }
    }

    // §FS-rhei-supervision.3.1: engine-owned release still consumes unread events.
    #[test]
    fn engine_release_with_pending_checkpoints_keeps_existing_semantics() {
        let machine = supervision_machine();
        let mut plan = supervised_plan(&["completed"]);
        let key = parse_task_id("1");
        let pending = checkpoint("1.1", "review", "completed", 1);
        plan.metadata = Some(record_supervision_hold(plan.metadata.as_ref(), &key, Some(&pending)));
        let before = plan.metadata.clone();
        let error = manual_release_preflight(
            &machine,
            plan.metadata.as_ref(),
            &key,
            "plan.1",
            "supervising",
            "supervising",
            false,
        )
        .unwrap_err()
        .to_string();
        for expected in [
            "plan.1.1 review -> completed",
            "1 unread checkpoint",
            "rhei retry <plan> --task plan.1",
            "--release-subtree",
        ] {
            assert!(error.contains(expected), "{error}");
        }
        assert_eq!(plan.metadata, before);
        assert_eq!(
            manual_release_preflight(
                &machine,
                plan.metadata.as_ref(),
                &key,
                "plan.1",
                "supervising",
                "supervising",
                true
            )
            .unwrap(),
            1
        );
        let task = find_task_by_id(&plan.tasks, &key).unwrap();
        let released = apply_supervision_transition(
            plan.metadata.as_ref(),
            SupervisionTransition {
                machine: &machine,
                task,
                supervising_owner: None,
                metadata_key: &key,
                metadata_prefix: "",
                local_id: "1",
                from: "supervising",
                to: "supervising",
                to_visit: 1,
                operation_supervisor: None,
            },
        )
        .unwrap();
        assert_eq!(supervision_phase(Some(&released), &key), SupervisionPhase::Released);
        assert!(supervision_checkpoints(Some(&released), &key).is_empty());
    }
}
