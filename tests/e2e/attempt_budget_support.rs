// Migration supplies outer headroom without changing the retry assertions.
// §FS-rhei-budgets.12 §FS-rhei-agents.3.2.3

fn attempt_home(plan: &Path) -> PathBuf {
    let root = plan.parent().unwrap();
    root.parent().unwrap().join(format!(".home-{}", root.file_name().unwrap().to_string_lossy()))
}

fn migrate_attempt_case(root: &Path, plan: &Path, machine: &Path) {
    let settings = root.join(".agent-grounds/rhei/settings.json");
    let raw = fs::read_to_string(&settings).unwrap();
    // Preserve the text prefix used by the attempts-default precedence cases.
    let raw = raw.replace("\"agent_timeout\": \"10s\"", "\"agent_timeout\": \"10s\", \"model\": \"fixture\", \"budget_threshold\": {\"currency\": \"USD\", \"amount_micro\": 2000}");
    let raw = raw.trim_end().strip_suffix('}').unwrap();
    fs::write(&settings, format!("{raw},\n\"models\": {{\"fixture\": {{\"provider\": \"rhei-test\", \"model\": \"fixed-2000\"}}}}\n}}\n")).unwrap();
    let mut document: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(machine).unwrap()).unwrap();
    let definitions = document["states"].as_mapping_mut().unwrap();
    let mut initial = None;
    let mut states = Vec::new();
    for (name, state) in definitions {
        let name = name.as_str().unwrap().to_owned();
        if state.as_mapping_mut().unwrap().remove(serde_yaml::Value::from("initial"))
            == Some(serde_yaml::Value::Bool(true))
        {
            assert!(initial.replace(name.clone()).is_none(), "one initial state per fixture");
        }
        states.push(name);
    }
    let initial = initial.expect("retry fixture declares its intended initial state");
    let mut definition = serde_yaml::to_string(&document).unwrap();
    let allowed = serde_json::to_string(&states).unwrap();
    definition.push_str(&format!("\nprofiles:\n  bounded:\n    initial: {initial}\n    allowed: {allowed}\n    transition_limit: 100\nnode_policy:\n  root: bounded\n  default: bounded\n"));
    fs::write(machine, definition).unwrap();
    super::budget_test_support::write_fixture_qualification(root, [true; 4]);
    let qualification = root.join(".agent-grounds/rhei/qualifications/rhei-test-fixed-2000.json");
    let text = fs::read_to_string(&qualification)
        .unwrap()
        .replace("\"agent\": \"codex\"", "\"agent\": \"mock\"");
    fs::write(qualification, text).unwrap();
    let mut command = rhei_command(attempt_home(plan));
    command.arg("--state-machine").arg(machine).args(["budget", "init"]).arg(plan).args([
        "--invocations",
        "100",
        "--spend-micro",
        "200000",
        "--currency",
        "USD",
        "--reason",
        "retry fixture outer headroom",
    ]);
    assert_success(&CliRun::from(&super::budget_test_support::bounded_budget_output(&mut command)));
}

fn run_cli(subcommand: &str, plan: &Path, machine: &Path, args: &[&str]) -> CliRun {
    let binary = if subcommand == "run" { budget_fixture_binary() } else { rhei_binary() };
    let mut command = rhei_process_at(binary);
    command
        .env("HOME", attempt_home(plan))
        .env("XDG_STATE_HOME", attempt_home(plan).join("state"))
        .arg("--state-machine")
        .arg(machine)
        .arg(subcommand)
        .arg(plan)
        .args(args);
    CliRun::from(&super::budget_test_support::bounded_budget_output(&mut command))
}
