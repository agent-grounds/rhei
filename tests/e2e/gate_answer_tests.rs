//! Black-box contract for answering a gate with `rhei gate answer`: it moves
//! the ticket where the routes say and records the answer, a dry run writes
//! nothing, every validation error comes back at once, and of two answers to
//! one visit exactly one wins. §FS-rhei-gate-cmd.3 §FS-rhei-gate-brief.7

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use super::gate_support::*;
use super::*;

/// `rhei gate answer <ticket> --from <from> --answers <file> --json`, with the
/// answer written to a file first. A refusal exits 1 and still prints the
/// outcome on stdout, so both are returned. §FS-rhei-gate-cmd.3.2
pub fn answer_json(
    fx: &GateFixture,
    ticket: &str,
    from: &str,
    answer: &Value,
    extra: &[&str],
) -> (CliRun, Value) {
    let file = fx.root().join(format!("answer-{ticket}-{from}.json"));
    fs::write(&file, answer.to_string()).expect("answer file");
    let file = file.display().to_string();
    let mut args =
        vec!["gate", "answer", ticket, "--from", from, "--answers", file.as_str(), "--json"];
    args.extend_from_slice(extra);
    let run = fx.rhei(&args);
    let outcome = serde_json::from_str(&run.stdout).unwrap_or_else(|err| {
        panic!("the outcome is one JSON object: {err}\n{}\n{}", run.stdout, run.stderr)
    });
    (run, outcome)
}

pub fn gate_schema(fx: &GateFixture, name: &str) -> Value {
    let result = fx.rhei(&["gate", "schema", name]);
    assert_success(&result);
    serde_json::from_str(&result.stdout).expect("a gate schema is JSON")
}

fn digest_of(fx: &GateFixture, ticket: &str) -> Value {
    fx.gate_json(&["show", ticket])["digest"].clone()
}

/// Every file under `root` but the fixture's home and the plan's lock, with
/// its contents: what "writes nothing" is held to.
pub fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).expect("readable fixture dir").flatten() {
            let path = entry.path();
            let name = path.strip_prefix(root).expect("under root").display().to_string();
            if name == ".home" || name.ends_with(".lock") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(name, fs::read(&path).expect("readable fixture file"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// The answer a route reads moves the ticket, the outcome says where and how,
/// and the file records who answered what, for the states after.
/// §FS-rhei-gate-cmd.3.2 §FS-rhei-gate-brief.7.1 §FS-rhei-gate-brief.7.2
#[test]
fn an_answer_moves_the_ticket_and_records_who_answered_what() {
    let fx = gate_fixture("gate-answer-moves");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");
    let digest = digest_of(&fx, "verdict.1");
    let sent = json!({ "version": 1, "request_digest": digest, "answers": { "approve": true, "note": "On Monday." } });

    let (run, outcome) = answer_json(&fx, "verdict.1", "review", &sent, &[]);
    assert_success(&run);
    let path = "runtime/gates/verdict.1/review.1.answer.json";
    let result = format!("Ship it?: yes\nAnything else?: On Monday.\nAnswer: {path}");
    assert_eq!(
        outcome,
        json!({
            "version": 1, "ok": true, "dry_run": false, "ticket": "verdict.1", "from": "review",
            "to": "completed", "route": { "index": 0, "when": { "approve": true } },
            "answer_path": path, "result": result, "errors": []
        })
    );
    assert_eq!(top_level_keys(&run.stdout)[..3], ["version", "ok", "dry_run"]);
    assert_matches_schema(&gate_schema(&fx, "result"), &outcome, "the answer's outcome");
    assert_eq!(fx.state_of("verdict.1"), "completed");

    let recorded: Value =
        serde_json::from_str(&fs::read_to_string(fx.root().join(path)).expect("answer file"))
            .expect("the answer file is JSON");
    assert_matches_schema(&gate_schema(&fx, "answer"), &recorded, "the recorded answer");
    assert_eq!(recorded["ticket"], "verdict.1");
    assert_eq!(
        (&recorded["state"], &recorded["visit"], &recorded["to"]),
        (&json!("review"), &json!(1), &json!("completed"))
    );
    assert_eq!(recorded["route"], outcome["route"]);
    assert_eq!(recorded["request_digest"], digest);
    assert_eq!(recorded["answers"], sent["answers"]);
    assert!(
        recorded["answered_by"].as_str().is_some_and(|who| !who.trim().is_empty()),
        "{recorded:#}"
    );
    let at = recorded["answered_at"].as_str().expect("answered_at");
    assert!(at.len() == 20 && at.ends_with('Z') && at.as_bytes()[10] == b'T', "RFC 3339 UTC: {at}");

    // The rendered answers are the move's result, as `--result` would be. §FS-rhei-states.3.3
    let results =
        fs::read_to_string(fx.root().join("runtime/results/verdict.1.md")).expect("result file");
    assert!(results.contains(&result), "{results}");
}

/// Static questions with a fallback route, read by a person: the fallback
/// takes the answer no earlier route matched, and the text output says where
/// the ticket went. A plain gate asks where, and why. §FS-rhei-gate-brief.5
/// §FS-rhei-gate-brief.4.1 §FS-rhei-gate-cmd.3
#[test]
fn a_fallback_route_and_a_plain_gate_take_their_answers() {
    let fx = gate_fixture("gate-answer-fallback");
    fx.enter("verdict.2", "vetting");
    let answer = fx.root().join("vetting.json");
    let sent = json!({ "version": 1, "request_digest": digest_of(&fx, "verdict.2"), "answers": { "approve": false } });
    fs::write(&answer, sent.to_string()).expect("answer");
    let run = fx.rhei(&[
        "gate",
        "answer",
        "verdict.2",
        "--from",
        "vetting",
        "--answers",
        &answer.display().to_string(),
    ]);
    assert_success(&run);
    assert!(
        run.stdout.contains("Task verdict.2 transitioned: 'vetting' → 'drafting'"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("Go ahead?: no\nAnswer: runtime/gates/verdict.2/vetting.1.answer.json"),
        "{}",
        run.stdout
    );
    assert_eq!(fx.state_of("verdict.2"), "drafting");

    fx.enter("verdict.3", "parked");
    let sent = json!({
        "version": 1, "request_digest": digest_of(&fx, "verdict.3"),
        "answers": { "destination": "completed", "reason": "It shipped elsewhere." }
    });
    let (run, outcome) = answer_json(&fx, "verdict.3", "parked", &sent, &[]);
    assert_success(&run);
    assert_eq!((&outcome["to"], &outcome["route"]), (&json!("completed"), &Value::Null));
    assert_eq!(
        outcome["result"],
        "Where should verdict.3 go?: completed\nWhy?: It shipped elsewhere.\n\
         Answer: runtime/gates/verdict.3/parked.1.answer.json"
    );
    assert_eq!(fx.state_of("verdict.3"), "completed");
}

/// A dry run says what the answer would do and changes nothing: no answer
/// file, no state, no ledger, no result. Its refusals are the real ones.
/// §FS-rhei-gate-cmd.3.1
#[test]
fn a_dry_run_checks_the_answer_and_writes_nothing() {
    let fx = gate_fixture("gate-answer-dry-run");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");
    let digest = digest_of(&fx, "verdict.1");
    let before = tree(fx.root());

    let sent = json!({ "version": 1, "request_digest": digest, "answers": { "approve": false } });
    let (run, outcome) = answer_json(&fx, "verdict.1", "review", &sent, &["--dry-run"]);
    assert_success(&run);
    assert_eq!(
        (&outcome["ok"], &outcome["dry_run"], &outcome["to"]),
        (&json!(true), &json!(true), &json!("drafting"))
    );
    assert_eq!(outcome["route"], json!({ "index": 1, "when": { "approve": false } }));
    assert_matches_schema(&gate_schema(&fx, "result"), &outcome, "a dry run's outcome");

    let stale =
        json!({ "version": 1, "request_digest": "sha256:0", "answers": { "approve": false } });
    let (refused, outcome) = answer_json(&fx, "verdict.1", "review", &stale, &["--dry-run"]);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!((&outcome["ok"], &outcome["errors"][0]["kind"]), (&json!(false), &json!("stale")));

    let mut after = tree(fx.root());
    after.retain(|name, _| !name.starts_with("answer-"));
    let mut before = before;
    before.retain(|name, _| !name.starts_with("answer-"));
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>(),
        "a dry run makes no file"
    );
    assert_eq!(after, before, "a dry run changes no file");
    assert_eq!(fx.state_of("verdict.1"), "review");
}

/// One refusal names every problem: a stale digest beside each answer that
/// does not fit, by question; nothing is written and the ticket stays. The
/// text form prints the same errors, one per line. §FS-rhei-gate-brief.6.2
/// §FS-rhei-gate-cmd.3.2
#[test]
fn every_validation_error_is_reported_together_and_nothing_is_written() {
    let fx = gate_fixture("gate-answer-invalid");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");
    let sent = json!({ "version": 1, "request_digest": "sha256:old", "answers": { "approve": "yes", "who": 1 } });

    let (run, outcome) = answer_json(&fx, "verdict.1", "review", &sent, &[]);
    assert_eq!(run.status.code(), Some(1), "{}", run.stderr);
    assert_matches_schema(&gate_schema(&fx, "result"), &outcome, "a refusal");
    assert_eq!(
        (&outcome["ok"], &outcome["to"], &outcome["route"], &outcome["result"]),
        (&json!(false), &Value::Null, &Value::Null, &Value::Null)
    );
    assert_eq!(outcome["answer_path"], "runtime/gates/verdict.1/review.1.answer.json");
    let errors: Vec<(String, Value)> = outcome["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|error| (error["kind"].as_str().expect("kind").to_string(), error["question"].clone()))
        .collect();
    assert_eq!(
        errors,
        [
            ("stale".into(), Value::Null),
            ("invalid".into(), json!("who")),
            ("invalid".into(), json!("approve"))
        ]
    );
    assert!(
        !fx.root().join("runtime/gates/verdict.1").exists(),
        "a refused answer records nothing"
    );
    assert_eq!(fx.state_of("verdict.1"), "review");

    let shapeless = fx.root().join("shapeless.json");
    fs::write(&shapeless, r#"{ "answers": [] }"#).expect("answer");
    let run = fx.rhei(&[
        "gate",
        "answer",
        "verdict.1",
        "--from",
        "review",
        "--answers",
        &shapeless.display().to_string(),
    ]);
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(run.stdout, "");
    let lines: Vec<&str> = run.stderr.lines().collect();
    assert_eq!(lines.len(), 3, "one line per error:\n{}", run.stderr);
    assert!(lines.iter().all(|line| line.starts_with("invalid: ")), "{}", run.stderr);
}

/// The two refusals about where the ticket is: a `--from` that is no gate,
/// and a ticket that is not in it. §FS-rhei-gate-brief.6.2
#[test]
fn a_state_that_is_no_gate_and_a_ticket_elsewhere_are_refused() {
    let fx = gate_fixture("gate-answer-where");
    let sent = json!({ "version": 1, "request_digest": "sha256:0", "answers": {} });
    let (run, outcome) = answer_json(&fx, "verdict.1", "drafting", &sent, &[]);
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(outcome["errors"][0]["kind"], "not_a_gate");
    assert_eq!(outcome["answer_path"], Value::Null);
    let (run, outcome) = answer_json(&fx, "verdict.1", "review", &sent, &[]);
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(outcome["errors"][0]["kind"], "conflict");
    assert!(
        outcome["errors"][0]["message"].as_str().is_some_and(|text| text.contains("'drafting'")),
        "{outcome:#}"
    );
}

/// Two answers to one visit, sent at once: exactly one moves the ticket and
/// is recorded, and the other is a `conflict` that writes nothing. Three
/// visits, so the race is run more than once. §FS-rhei-gate-cmd.3
#[test]
fn of_two_answers_sent_at_once_exactly_one_wins() {
    let fx = gate_fixture("gate-answer-race");
    for visit in 1..=3 {
        fx.prepare_review("verdict.1", GOOD_REQUEST);
        fx.enter("verdict.1", "review");
        let digest = digest_of(&fx, "verdict.1");
        let children: Vec<_> = ["first", "second"]
            .iter()
            .map(|who| {
                let file = fx.root().join(format!("race-{who}.json"));
                let sent = json!({ "version": 1, "request_digest": digest, "answers": { "approve": false, "note": who } });
                fs::write(&file, sent.to_string()).expect("answer");
                rhei_command(&fx.home)
                    .current_dir(fx.root())
                    .args(["gate", "answer", "verdict.1", "--from", "review", "--json", "--answers"])
                    .arg(&file)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .expect("rhei should start")
            })
            .collect();
        let outcomes: Vec<(bool, Value)> = children
            .into_iter()
            .map(|child| {
                let output = child.wait_with_output().expect("rhei should finish");
                let outcome = serde_json::from_slice(&output.stdout).expect("an outcome on stdout");
                (output.status.success(), outcome)
            })
            .collect();
        let winners: Vec<&Value> =
            outcomes.iter().filter(|(won, _)| *won).map(|(_, outcome)| outcome).collect();
        assert_eq!(winners.len(), 1, "visit {visit}: {outcomes:#?}");
        let loser = &outcomes.iter().find(|(won, _)| !won).expect("a loser").1;
        assert_eq!(loser["errors"][0]["kind"], "conflict", "visit {visit}: {loser:#}");

        let path = format!("runtime/gates/verdict.1/review.{visit}.answer.json");
        let recorded: Value = serde_json::from_str(
            &fs::read_to_string(fx.root().join(&path)).expect("the winner's file"),
        )
        .expect("answer JSON");
        let winner_note = winners[0]["result"]
            .as_str()
            .expect("result")
            .lines()
            .nth(1)
            .expect("note line")
            .to_string();
        assert_eq!(
            format!("Anything else?: {}", recorded["answers"]["note"].as_str().expect("note")),
            winner_note
        );
        assert_eq!(fx.state_of("verdict.1"), "drafting");
    }
}
