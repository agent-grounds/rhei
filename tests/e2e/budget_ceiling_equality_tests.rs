//! Port of rhei.94's ping-pong reproduction: an inner request at its ceiling
//! cannot buy another move, while a request below it can.
//! §FS-rhei-budgets.8 §FS-rhei-budgets.2.3 §FS-rhei-run-json.2.1

use std::fs;
use std::path::Path;

use super::budget_delegated_support::*;
use super::budget_support::*;
use super::*;

const PROJECT_HOME: &str = ".agent-grounds/rhei";

#[derive(Clone, Copy)]
enum Requester {
    Project,
    Profile,
    DelegatedProfile,
}

impl Requester {
    fn source(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Profile | Self::DelegatedProfile => "plan",
        }
    }

    fn write(self, root: &Path, states: &Path, value: u32) {
        let profile = match self {
            Self::Project => None,
            Self::Profile | Self::DelegatedProfile => Some(value),
        };
        fs::write(states, profile_machine(profile)).expect("write profile request");
        let project = match self {
            Self::Project => value,
            Self::Profile => return,
            Self::DelegatedProfile => 4,
        };
        write_project_settings(
            root,
            PROJECT_HOME,
            &format!(r#"{{ "defaults": {{ "transition_limit": {project} }} }}"#),
        );
    }

    fn remedy(self, root: &Path, at_ceiling: bool) -> Vec<String> {
        let file = if at_ceiling {
            match self {
                Self::Project | Self::Profile => machine_file(root),
                Self::DelegatedProfile => project_file(root, PROJECT_HOME),
            }
        } else if matches!(self, Self::Project) {
            project_file(root, PROJECT_HOME)
        } else {
            return vec!["set `transition_limit` on the node's profile in the state machine".into()];
        };
        spellings(&file)
            .into_iter()
            .map(|path| format!("set `defaults.transition_limit` in {path}"))
            .collect()
    }
}

fn run(root: &Path, plan: &Path, states: &Path, json: bool) -> CliRun {
    let home = home_for(root);
    let mut cmd = rhei_command(&home);
    // Nested fixture runs must not inherit the enclosing worker's reservation.
    for (key, _) in
        std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("RHEI_"))
    {
        cmd.env_remove(key);
    }
    cmd.env("XDG_CONFIG_HOME", home.join(".config")).env("RHEI_BUDGET_NOW", DAY);
    cmd.arg("--state-machine").arg(states).arg("run").arg(plan);
    cmd.args(["--no-tui", "--no-callbacks"]);
    if json {
        cmd.arg("--json");
    }
    CliRun::from(&cmd.output().expect("run isolated ping-pong fixture"))
}

fn halt(result: &CliRun, effective: u32, source: &str, limiter: Option<&str>) -> serde_json::Value {
    assert!(!result.status.success(), "a spent travel bound halts the run");
    let event = result
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON run record"))
        .find(|record| record["event"] == "budget_halt")
        .unwrap_or_else(|| panic!("no budget_halt: {}", combined(result)));
    assert_eq!(event["dimension"], "ticket travel", "{event:#}");
    assert_eq!(event["effective"], effective, "{event:#}");
    assert_eq!(event["value_source"], source, "{event:#}");
    assert_eq!(event["limiting_source"], serde_json::json!(limiter), "{event:#}");
    event
}

fn check_remedies(requester: Requester) {
    let machine_extra = if matches!(requester, Requester::DelegatedProfile) {
        r#", "transition_limit": 3, "clamp_projects": false"#
    } else {
        r#", "transition_limit": 4"#
    };

    // Control: raising the inner request from three to four buys one more move.
    let (control, plan, states) =
        setup("ceiling-equality-control", &profile_machine(None), machine_extra);
    requester.write(&control, &states, 3);
    let human = run(&control, &plan, &states, false);
    assert_spawn_count(&control, 3, "a below-ceiling request buys three moves");
    assert_halt_mentions(&human, &format!("bound:       3 ({})", requester.source()));
    let json = halt(&run(&control, &plan, &states, true), 3, requester.source(), None);
    let remedies = requester.remedy(&control, false);
    assert!(remedies.iter().any(|r| combined(&human).contains(&format!("to raise it: {r}"))));
    assert!(remedies.iter().any(|r| json["remedy"] == r.as_str()), "{json:#}");
    requester.write(&control, &states, 4);
    halt(&run(&control, &plan, &states, true), 4, requester.source(), None);
    assert_spawn_count(&control, 4, "raising a below-ceiling request adds one move");

    let (dir, plan, states) = setup("ceiling-equality", &profile_machine(None), machine_extra);
    requester.write(&dir, &states, 4);
    let human = run(&dir, &plan, &states, false);
    assert_spawn_count(&dir, 4, "an equal request buys exactly four moves");
    assert_halt_mentions(&human, &format!("bound:       4 ({})", requester.source()));
    let json = halt(&run(&dir, &plan, &states, true), 4, requester.source(), None);

    // Following the old inner-value remedy cannot raise the effective bound.
    requester.write(&dir, &states, 100);
    let limiter =
        if matches!(requester, Requester::DelegatedProfile) { "project" } else { "machine" };
    let raised = halt(&run(&dir, &plan, &states, true), 4, requester.source(), Some(limiter));
    assert_spawn_count(&dir, 4, "raising only the inner request to 100 buys no further move");
    let remedies = requester.remedy(&dir, true);
    assert!(remedies.iter().any(|r| raised["remedy"] == r.as_str()), "{raised:#}");

    let text = combined(&human);
    let human_correct = remedies.iter().any(|r| text.contains(&format!("to raise it: {r}")));
    let json_correct = remedies.iter().any(|r| json["remedy"] == r.as_str());
    assert!(
        human_correct && json_correct,
        "at equality both remedies must name the active ceiling's settings path\n\
         expected one of: {remedies:?}\nhuman correct: {human_correct}\n\
         JSON correct: {json_correct}\nhuman:\n{text}\nJSON budget_halt:\n{json:#}"
    );
}

#[test]
fn ceiling_equality_project_request_names_machine_in_human_and_json_halts() {
    check_remedies(Requester::Project);
}

#[test]
fn ceiling_equality_profile_request_names_machine_in_human_and_json_halts() {
    check_remedies(Requester::Profile);
}

#[test]
fn ceiling_equality_profile_request_names_delegated_project_in_human_and_json_halts() {
    check_remedies(Requester::DelegatedProfile);
}
