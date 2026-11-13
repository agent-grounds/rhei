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

/// `--parallel` is how a user sizes a run against a rate limit or a spend in
/// flight, so its one help line must name the unit it actually counts —
/// tasks — and say that a task's `all_targets` / `all_models` arms all run
/// alongside it. Calling the unit agents promised `--parallel 1` one agent
/// where a two-arm state runs two (#673). §FS-rhei-run.5.3
#[test]
fn run_help_says_parallel_counts_tasks_and_their_fan_out_arms_run_alongside() {
    let mut command = cli_command();
    let run = command.find_subcommand_mut("run").expect("`run` subcommand");

    // The listing `rhei run --help` prints for the option: its first paragraph.
    let description = run
        .get_arguments()
        .find(|arg| arg.get_id() == "parallel")
        .expect("`run` declares `--parallel`")
        .get_help()
        .expect("`--parallel` carries help")
        .to_string();

    assert!(
        description.contains("tasks"),
        "`--parallel` should name tasks as the unit it counts, got: {description}"
    );
    assert!(
        !description.contains("number of agents"),
        "`--parallel` must not call its unit agents: a fanned-out task runs one agent per \
         arm, got: {description}"
    );
    assert!(
        description.contains("all_targets") && description.contains("all_models"),
        "`--parallel` should say that a task's `all_targets` / `all_models` arms run \
         alongside it, got: {description}"
    );
    assert!(
        description.contains("0 = unlimited"),
        "`--parallel` keeps saying what 0 means, got: {description}"
    );

    let help = run.render_help().to_string();
    assert!(
        help.contains("[default: 1]"),
        "`--parallel` keeps its default of one task:\n{help}"
    );
}
