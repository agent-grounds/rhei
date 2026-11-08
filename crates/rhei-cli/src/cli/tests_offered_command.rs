// A shell command a help offers as the next action is recognized by the words
// that offer it, and only by them. §FS-rhei-errors.2.1

mod offered_command {
    use super::*;

    /// Every lead-in, followed by a command, is an offer the renderer keeps whole.
    #[test]
    fn offered_command_every_lead_in_is_recognized() {
        for lead_in in OFFERED_COMMAND_LEAD_INS {
            let help = format!("something is wrong. {lead_in}cmd 'some/long path'");
            assert!(offers_a_command(&help), "{lead_in:?} should be recognized in {help:?}");
        }
    }

    /// The help each site builds is recognized, so a reworded site cannot fall out
    /// of the guard while its format string still uses the constant.
    #[test]
    fn offered_command_each_filesystem_help_is_recognized() {
        let missing = Path::new("no-such-directory/nested/plan.rhei.md");
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::AlreadyExists,
        ] {
            let help = io_error_help(missing, kind);
            assert!(offers_a_command(&help), "{kind:?}: {help:?}");
        }
    }

    /// A help that names no command is prose, and keeps wrapping.
    #[test]
    fn offered_command_help_with_no_command_is_not_an_offer() {
        for help in [
            "check that 'some/dir' exists, is writable, and has free space.",
            "check that the path exists and is writable.",
            "nothing exists at that path. Check the spelling.",
            "the description is read from this path; check that it exists and is readable.",
        ] {
            assert!(!offers_a_command(help), "{help:?} offers no command");
        }
    }

    /// Recognizing an offered shell command leaves the Rhei-target rule as it was:
    /// each reads its own text, and neither stands in for the other.
    #[test]
    fn offered_command_rhei_target_recognition_is_unchanged() {
        let targeted = "inspect it with: rhei validate 'sub dir/plan.rhei.md'";
        assert!(names_help_target(targeted));
        assert!(!offers_a_command(targeted));
        let untargeted = "List them with: rhei states";
        assert!(!names_help_target(untargeted));
        assert!(!offers_a_command(untargeted));
        let offered = "Create it with: mkdir -p 'sub dir'";
        assert!(offers_a_command(offered));
        assert!(!names_help_target(offered));
    }

    /// The core budget refusals word their lead-ins differently from the CLI's, and
    /// both are recognized. §FS-rhei-budgets.5.4.2 §FS-rhei-budgets.5.4.3
    #[test]
    fn offered_command_core_budget_lead_ins_are_recognized() {
        assert!(offers_a_command(
            "copying the witness back would import them, so set the witness aside instead, \
             keeping every byte: mv a b"
        ));
        assert!(offers_a_command(
            "so keep it and cut it back to its last whole receipt with: cp a b && truncate b"
        ));
    }
}
