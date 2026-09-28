//! Black-box JSON contract for `rhei show`: exactly three fields, and a
//! `content` that is the text form's body to the byte. §FS-rhei-show.4

use std::collections::BTreeSet;

use serde_json::Value;

use super::show_support::*;
use super::*;

fn show_json(result: &CliRun) -> Value {
    assert_success(result);
    serde_json::from_str(&result.stdout).unwrap_or_else(|error| {
        panic!(
            "rhei show --json must emit one JSON object: {error}\nstdout:\n{}\nstderr:\n{}",
            result.stdout, result.stderr
        )
    })
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value.as_object().expect("JSON value should be an object").keys().map(String::as_str).collect()
}

/// Three fields, and no fourth. `state`, `kind`, `assignee`, `prior`, `parent`
/// and `depth` are `rhei list --json`'s promise, and two surfaces that must
/// agree about a ticket's state is one too many. §FS-rhei-show.4
#[test]
fn show_json_emits_exactly_the_three_fields() {
    let (dir, _plan) = show_fixture("show-json-fields");
    let home = dir.join(".home");

    let result = run_show(&home, &dir, &["probe.2", "--json"]);
    let payload = show_json(&result);

    assert_eq!(keys(&payload), BTreeSet::from(["content", "id", "title"]));
    assert_eq!(payload["id"], "probe.2");
    assert_eq!(payload["title"], "A finished ticket nobody may claim");
    assert_eq!(result.stderr, "", "the machine form writes nothing to stderr");
}

/// `content` is what the text form prints below the heading, byte for byte, so a
/// reader and a script are looking at the same text. §FS-rhei-show.4
#[test]
fn show_json_content_is_byte_identical_to_the_text_body() {
    let (dir, _plan) = show_fixture("show-json-identical");
    let home = dir.join(".home");

    let text = run_show(&home, &dir, &["probe.2"]);
    assert_success(&text);
    let payload = show_json(&run_show(&home, &dir, &["probe.2", "--json"]));

    let content = payload["content"].as_str().expect("content is a string");
    assert_eq!(content, TICKET_2_BODY);
    assert_eq!(
        text.stdout,
        format!(
            "## Task probe.2: {}\n\n{content}\n",
            payload["title"].as_str().expect("title is a string")
        )
    );
}

/// An empty body is `""` rather than a missing key: the field set is the
/// contract, and a ticket with nothing under it still has all three.
/// §FS-rhei-show.4
#[test]
fn show_json_carries_an_empty_content_for_an_empty_body() {
    let (dir, _plan) = show_fixture("show-json-empty");
    let home = dir.join(".home");

    let payload = show_json(&run_show(&home, &dir, &["probe.3", "--json"]));

    assert_eq!(keys(&payload), BTreeSet::from(["content", "id", "title"]));
    assert_eq!(payload["content"], "");
}

/// A failing `show --json` is §FS-rhei-errors.5's single-line object on stderr,
/// not miette prose: the verb whose whole point is that a script can read it
/// must not hand that script two shapes. §FS-rhei-errors.5
#[test]
fn show_json_reports_a_missing_ticket_as_a_json_error() {
    let (dir, _plan) = show_fixture("show-json-error");
    let home = dir.join(".home");

    let result = run_show(&home, &dir, &["probe.99", "--json"]);

    assert!(!result.status.success(), "an unknown ticket should fail: {}", result.stdout);
    assert_eq!(result.stdout, "", "nothing but the object may reach stdout");
    let payload: Value = serde_json::from_str(result.stderr.trim()).unwrap_or_else(|error| {
        panic!("stderr should be one JSON object: {error}\nstderr:\n{}", result.stderr)
    });
    assert!(
        payload["error"]["message"].as_str().is_some_and(|message| message.contains("probe.99")),
        "got:\n{}",
        result.stderr
    );
    assert!(
        payload["error"]["help"].as_str().is_some_and(|help| help.contains("rhei list")),
        "expected the help to travel with the JSON error; got:\n{}",
        result.stderr
    );
}
