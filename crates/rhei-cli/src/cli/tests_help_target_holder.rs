// The one place a help's target comes from: what it answers on the thread a help
// is built on, and on no other. §FS-rhei-errors.1.2.1

mod help_target_holder {
    use super::*;

    /// A parallel worker reports from its own thread, which starts with nothing.
    #[test]
    fn help_target_a_help_built_on_a_worker_thread_names_the_command_target() {
        let _scope = enter_help_target(HelpTarget::new(
            Some(PathBuf::from("sub dir/plan.rhei.md")),
            true,
            Some(PathBuf::from("machine.yaml")),
        ));
        let carried = current_help_target();

        let built = std::thread::spawn(move || {
            inherit_help_target(carried);
            task_id_help()
        })
        .join()
        .expect("the worker builds its help");

        assert_eq!(
            built,
            format!(
                "list the task ids in this plan with: rhei list {} --state-machine machine.yaml",
                shell_quote("sub dir/plan.rhei.md")
            )
        );
    }

    /// Nothing leaks to a thread the target was not carried to, which is what keeps
    /// one unit test's target out of another's.
    #[test]
    fn help_target_is_not_seen_on_a_thread_it_was_not_carried_to() {
        let _scope = enter_help_target(HelpTarget::naming(Path::new("mine.rhei.md")));

        let built = std::thread::spawn(task_id_help).join().expect("the thread builds its help");

        assert_eq!(built, "list the task ids in this plan with: rhei list <plan>");
    }

    /// A scope ends and the thread holds what it held, nested or not.
    #[test]
    fn help_target_scope_restores_what_it_replaced() {
        assert_eq!(help_target_word(), "<plan>");
        {
            let _outer = enter_help_target(HelpTarget::naming(Path::new("outer.rhei.md")));
            {
                let _inner = enter_help_target(HelpTarget::naming(Path::new("inner.rhei.md")));
                assert_eq!(help_target_word(), "inner.rhei.md");
            }
            assert_eq!(help_target_word(), "outer.rhei.md");
        }
        assert_eq!(help_target_word(), "<plan>");
    }

    /// `rhei instantiate` and its kin have no target of their own: a help about a
    /// path they validated keeps that path. A plan command's typed target wins over
    /// a widened one. §FS-rhei-errors.1.2.1
    #[test]
    fn help_target_only_the_command_own_target_replaces_the_path_in_hand() {
        let in_hand = Path::new("the-path-it-validated");
        assert_eq!(help_target_word_or(in_hand), "the-path-it-validated");

        {
            let target = HelpTarget::of_command(&Commands::Version, None);
            target.discovered.set(Some(PathBuf::from("what-the-directory-holds"))).unwrap();
            let _scope = enter_help_target(target);
            assert_eq!(help_target_word_or(in_hand), "the-path-it-validated");
        }

        let _scope = enter_help_target(HelpTarget::naming(Path::new("member.rhei.md")));
        assert_eq!(help_target_word_or(Path::new("project")), "member.rhei.md");
    }

    /// A verb that runs with no target stays bare where the command has none, and
    /// what the directory holds is not the answer for a command with no target of its
    /// own, even once something has looked. §FS-rhei-errors.1.2.1
    #[test]
    fn help_target_a_verb_that_runs_bare_never_gains_a_placeholder() {
        assert_eq!(help_command_or_bare("states"), "rhei states");
        {
            let target = HelpTarget::new(None, false, None);
            target.discovered.set(Some(PathBuf::from("what-the-directory-holds"))).unwrap();
            let _scope = enter_help_target(target);
            assert_eq!(help_command_or_bare("states"), "rhei states");
            assert_eq!(help_command("list"), "rhei list <plan>");
        }
        {
            let machine = Some(PathBuf::from("machine.yaml"));
            let _scope = enter_help_target(HelpTarget::new(None, false, machine.clone()));
            assert_eq!(help_command_or_bare("states"), "rhei states --state-machine machine.yaml");

            let typed = Some(PathBuf::from("plan.rhei.md"));
            let _scope = enter_help_target(HelpTarget::new(typed, true, machine));
            assert_eq!(
                help_command_or_bare("states"),
                "rhei states plan.rhei.md --state-machine machine.yaml"
            );
        }
    }

    /// A verb that takes no `--state-machine` is not handed one. §FS-rhei-errors.1.2.1
    #[test]
    fn help_target_a_verb_with_no_machine_flag_is_not_handed_one() {
        let _scope = enter_help_target(HelpTarget::new(
            Some(PathBuf::from("plan.rhei.md")),
            true,
            Some(PathBuf::from("machine.yaml")),
        ));
        assert_eq!(
            help_command_without_machine("show", "--task 1", None),
            "rhei show plan.rhei.md --task 1"
        );
        assert_eq!(help_machine_argument(), " --state-machine machine.yaml");
    }

    /// A machine given before the subcommand is passed on the same as one after it,
    /// and to a command that takes more arguments it comes last.
    #[test]
    fn help_target_passes_on_the_machine_after_the_arguments() {
        let _scope = enter_help_target(HelpTarget::of_command(
            &Commands::Roster { input: Some(PathBuf::from("plan.rhei.md")), json: false },
            Some(Path::new("machine.yaml")),
        ));
        assert_eq!(
            help_command_with("list", "--non-terminal"),
            "rhei list plan.rhei.md --non-terminal --state-machine machine.yaml"
        );
    }

    /// What the renderer keeps on one line. A bare directory is only a target when it
    /// is the word the command was given, or `rhei note appends` would be one too.
    #[test]
    fn help_target_the_renderer_keeps_a_line_naming_a_target() {
        for named in [
            "re-check with: rhei validate plan.rhei.md",
            "See: rhei list 'sub dir/plan.rhei.md' --non-terminal",
            "rhei list /abs/plan.rhei.md, then claim it",
            "start one with `rhei run ./plan --dashboard`",
            "rhei transition plan.rhei.md --task 1 --state-machine m.yaml",
            "rhei intervene --plan project/plan.rhei.md --task 1 --message <text>",
            "rhei states --state-machine 'machines dir/custom.yaml'",
            "rhei states --state-machine custom.yaml",
        ] {
            assert!(names_help_target(named), "{named}");
        }
        for prose in [
            "see what is left with: rhei list <plan>",
            "rhei note appends to runtime/notes.md at the project execution root",
            "rhei transition auth.1 --from a --to b",
            "the rhei validate step",
            "library/rhei list plan.rhei.md",
            "rhei list",
        ] {
            assert!(!names_help_target(prose), "{prose}");
        }

        assert!(!names_help_target("rhei list project"));
        let _scope = enter_help_target(HelpTarget::naming(Path::new("project")));
        assert!(names_help_target("rhei list project"));
        assert!(names_help_target("See it with: rhei list project."));
    }
}
