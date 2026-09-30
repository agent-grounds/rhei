// The one CLI-side hole the path-reuse ticket left: a failing
// `budget show --format json` had no arm in `command_wants_json`, so a harness
// asking for JSON got rendered prose on the very state it was asking about.

// §FS-rhei-budgets.10 §FS-rhei-errors.5

fn budget_show_command_with(format: BudgetFormat) -> Commands {
    Commands::Budget {
        command: BudgetCommand::Show { input: None, rhei: Vec::new(), format },
    }
}

#[test]
fn budget_show_json_is_a_json_command() {
    assert!(
        command_wants_json(&budget_show_command_with(BudgetFormat::Json)),
        "a failing `budget show --format json` owes the machine shape"
    );
    assert!(
        !command_wants_json(&budget_show_command_with(BudgetFormat::Text)),
        "the text format keeps the rendered diagnostic it has always had"
    );
}

/// The arm is narrowed to `show`, because `init`, `adjust` and `forget` have
/// no `--format` and a blanket arm would silently change their failures.
#[test]
fn the_other_budget_subcommands_are_not_json_commands() {
    let forget = Commands::Budget {
        command: BudgetCommand::Forget { input: None, reason: "reused".into() },
    };
    assert!(!command_wants_json(&forget));
}
