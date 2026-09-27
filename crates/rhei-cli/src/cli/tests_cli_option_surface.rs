// What the rendered command-line surface promises about an option: that
// `--help` names it, and that what it says there is what the option does.
// §FS-rhei-transition-cmd.2

/// `--supervisor` is a documented option, not a hidden one: rhei's own
/// supervisor prompt instructs an agent to type it
/// (§FS-rhei-supervision.5.1), so a help listing that denies it leaves the
/// agent working out which of the two is wrong before it dares run the
/// command. §FS-rhei-transition-cmd.2
#[test]
fn transition_help_documents_the_supervisor_option() {
    let mut command = cli_command();
    let transition = command.find_subcommand_mut("transition").expect("`transition` subcommand");

    let help = transition.render_help().to_string();
    assert!(
        help.contains("--supervisor"),
        "`rhei transition --help` should list `--supervisor`:\n{help}"
    );

    // clap renders a doc comment's *first paragraph* as the option's short
    // help, so a blank line in it would leave the listing naming the flag
    // without saying what it does — half the answer.
    let description = transition
        .get_arguments()
        .find(|arg| arg.get_id() == "supervisor")
        .expect("`transition` declares `--supervisor`")
        .get_help()
        .expect("`--supervisor` carries help")
        .to_string();
    assert!(
        description.contains("checkpoint"),
        "`--supervisor` should be described by what it does — suppress the \
         checkpoint this move would deliver to the named supervisor — got: {description}"
    );
}

/// `rhei show` is a documented verb with a documented machine form: the listing
/// names the subcommand, its `--json` flag, and its `--task` alternative, because
/// every error this command raises tells a reader to type it. §FS-rhei-show.1
#[test]
fn show_help_documents_the_ticket_target_and_the_machine_form() {
    let mut command = cli_command();
    let show = command.find_subcommand_mut("show").expect("`show` subcommand");

    let help = show.render_help().to_string();
    for flag in ["--json", "--task"] {
        assert!(help.contains(flag), "`rhei show --help` should list `{flag}`:\n{help}");
    }

    let description = show
        .get_about()
        .expect("`show` carries an about line")
        .to_string();
    assert!(
        description.contains("body"),
        "`show` should be described by what it prints — one task's body — got: {description}"
    );
}
