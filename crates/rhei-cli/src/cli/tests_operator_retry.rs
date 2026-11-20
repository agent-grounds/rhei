// Round-trip the offered command through the host shell and the actual CLI parser.
// §FS-rhei-errors.1.2

fn issue_757_parse_retry(retry: &str) -> Cli {
    let python = if cfg!(windows) { "python" } else { "python3" };
    let capture = shell_quote("import json,sys; print(json.dumps(['rhei', *sys.argv[1:]]))");
    let command = format!("{python} -c {capture} {}", retry.strip_prefix("rhei ").unwrap());
    let output = rhei_core::platform::system_shell_command(&command).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let argv: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    Cli::try_parse_from(argv).unwrap_or_else(|error| panic!("{retry}: {error}"))
}

/// Option-like values, quoted operands and both selectors survive a shell paste.
/// §FS-rhei-errors.1.2
#[test]
fn issue_757_force_retry_round_trips_original_operands() {
    for positional in [false, true] {
        for (reason, result) in [("-repair", "-outcome"),
            ("owner's $(literal) route", "outcome with spaces and \"quotes\"")] {
            let input = Path::new("-plan with spaces.rhei.md");
            let machine = Path::new("-machine with spaces.yaml");
            let task = if positional { "profile.deliver" } else { "-selector" };
            let mut argv = ["rhei", "transition", "--from=-gate", "--to=-work", "--force"]
                .map(str::to_owned).to_vec();
            argv.extend([format!("--reason={reason}"), format!("--result={result}"),
                format!("--state-machine={}", machine.display())]);
            if !positional { argv.push(format!("--task={task}")); }
            argv.extend(["--".to_owned(), if positional { task.to_owned() } else { input.display().to_string() }]);
            let command = Cli::try_parse_from(argv).unwrap().command;
            let _scope = enter_help_target(HelpTarget::of_command(&command, None));
            let request = ForcedRequest { input, scope: &[], machine_path: Some(machine), task,
                from: "-gate", to: "-work", reason, result: Some(result) };
            let handoff = OperatorHandoff::force(&request, "profile.deliver");
            assert!(!handoff.retry.contains('\n'), "{}", handoff.retry);
            let parsed = issue_757_parse_retry(&handoff.retry);
            let Commands::Transition { input: actual_input, task: actual_task, from, to,
                force, reason: actual_reason, result: actual_result, state_machine, .. } = parsed.command
                else { panic!("expected transition"); };
            assert_eq!(actual_input, Some(if positional { PathBuf::from(task) } else { input.into() }));
            assert_eq!(actual_task.as_deref(), (!positional).then_some(task));
            assert_eq!((from.as_str(), to.as_str(), force), ("-gate", "-work", true));
            assert_eq!(actual_reason.as_deref(), Some(reason));
            assert_eq!(actual_result.as_deref(), Some(result));
            assert_eq!(state_machine.as_deref(), Some(machine));
            assert_eq!(handoff.expected, "force profile.deliver -gate -> -work");
        }
    }
}

/// Recovery retains the typed root even when it needs an option terminator.
/// §FS-rhei-errors.1.2 §FS-rhei-recover.3
#[test]
fn issue_757_recovery_retry_round_trips_original_root() {
    let (dir, plan, machine) = operator_fixture();
    let request = operator_request(&plan, &machine);
    interrupt_force_at("marker-after");
    assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
    clear_force_interrupt();
    let marker = recorded_marker(dir.path());
    for root in ["-root with spaces", "owner's $(literal) root"] {
        let command = Cli::try_parse_from(["rhei", "recover", "--", root]).unwrap().command;
        let _scope = enter_help_target(HelpTarget::of_command(&command, None));
        let handoff = OperatorHandoff::recovery(dir.path(), &marker, ForcedDecision::Rollback);
        assert!(!handoff.retry.contains('\n'), "{}", handoff.retry);
        let parsed = issue_757_parse_retry(&handoff.retry);
        let Commands::Recover { execution_root } = parsed.command else { panic!("expected recovery"); };
        assert_eq!(execution_root, PathBuf::from(root));
        assert_eq!(handoff.expected, format!("recover {} plan.1 gate -> work rollback", marker.recovery_id));
    }
}
