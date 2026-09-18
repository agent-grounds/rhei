// Target-aware dispatch for commands that validate the complete loaded graph.

// §AR-source-file-size.3 §FS-rhei-migrate.5

fn dispatch_target_command(
    command: Commands,
    before_subcommand: Option<PathBuf>,
) -> MietteResult<()> {
    match command {
        Commands::Migrate {
            command: MigrateCommand::ExportPriors { input, dry_run },
        } => {
            let target = resolve_plan_target(input)?;
            migrate_export_priors_command(target.path(), dry_run)
        }
        Commands::Validate { watch, input, state_machine } => {
            let requested = input.clone();
            // Validation never narrows: a member validates the project it
            // cannot resolve without. §FS-rhei-validate.1.1
            let target = resolve_plan_target(input)?;
            report_validation_widened(&target);
            let help_target = requested.as_deref().unwrap_or_else(|| target.path());
            let state_machine = state_machine.or(before_subcommand);
            with_validation_migration_target(help_target, || {
                validate_command(target.path(), state_machine.as_deref(), watch)
            })
        }
        Commands::Run { input, standalone, agent, program, snapshot, state_machine } => {
            let requested = input.clone();
            let target = resolve_plan_target(input)?;
            let mut options: RunOptions = (standalone, agent, program, snapshot).into();
            options.narrow_to(target.scope_with(options.rhei_scope()));
            let help_target = requested.as_deref().unwrap_or_else(|| target.path());
            let state_machine = state_machine.or(before_subcommand);
            with_validation_migration_target(help_target, || {
                run_command(target.path(), state_machine.as_deref(), options)
            })
        }
        _ => unreachable!("only target-aware commands are delegated here"),
    }
}
