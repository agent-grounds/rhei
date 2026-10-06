//! Reading, writing and numbering against the retirement record.
//! §FS-rhei-remove.5.1 §FS-rhei-new.4

use super::*;

fn frontmatter(yaml: &str) -> Metadata {
    serde_yaml::from_str(yaml).expect("test yaml")
}

#[test]
fn an_absent_or_empty_record_retires_nothing() {
    assert!(retired_tickets(None).expect("no metadata").is_empty());
    let empty = frontmatter("metadata:\n  retiredTickets:\n");
    assert!(retired_tickets(Some(&empty)).expect("null record").is_empty());
}

#[test]
fn reads_both_entry_shapes() {
    let meta = frontmatter(
        "metadata:\n  retiredTickets:\n    grund.63: {}\n    grund.61:\n      budgetTicketId: abc\n",
    );
    let retired = retired_tickets(Some(&meta)).expect("well formed");
    assert_eq!(retired["grund.63"], RetiredTicket::default());
    assert_eq!(retired["grund.61"].budget_ticket_id.as_deref(), Some("abc"));
}

#[test]
fn a_reserved_key_that_is_not_a_record_is_an_error() {
    for yaml in [
        "metadata:\n  retiredTickets: yes\n",
        "metadata:\n  retiredTickets: [a.1]\n",
        "metadata:\n  retiredTickets:\n    a.1: gone\n",
        "metadata:\n  retiredTickets:\n    a.1:\n      reason: typo\n",
        "metadata:\n  retiredTickets:\n    a.1:\n      budgetTicketId: 7\n",
    ] {
        let error = retired_tickets(Some(&frontmatter(yaml))).expect_err(yaml);
        assert!(error.contains("retiredTickets"), "{yaml}: {error}");
    }
}

#[test]
fn recording_keeps_every_other_key_and_entry() {
    let mut meta = frontmatter(
        "title: T\nmetadata:\n  tasks:\n    1:\n      stateVisits: {a: 1}\n  \
         retiredTickets:\n    auth.2: {}\n",
    );
    record_retirement(&mut meta, "auth.3", &RetiredTicket { budget_ticket_id: Some("u".into()) })
        .expect("record");
    let retired = retired_tickets(Some(&meta)).expect("still well formed");
    assert_eq!(retired.len(), 2);
    assert_eq!(retired["auth.3"].budget_ticket_id.as_deref(), Some("u"));
    assert_eq!(meta.get("title").and_then(Value::as_str), Some("T"));
    let tasks = meta["metadata"]["tasks"].as_mapping().expect("tasks kept");
    assert_eq!(tasks.len(), 1);
}

#[test]
fn recording_creates_the_record_where_there_was_none() {
    let mut meta = Metadata::new();
    record_retirement(&mut meta, "solo.1", &RetiredTicket::default()).expect("record");
    assert!(retired_tickets(Some(&meta)).expect("well formed").contains_key("solo.1"));
}

#[test]
fn recording_refuses_to_overwrite_a_foreign_value() {
    let mut meta = frontmatter("metadata:\n  retiredTickets: mine\n");
    assert!(record_retirement(&mut meta, "a.1", &RetiredTicket::default()).is_err());
    assert_eq!(meta["metadata"]["retiredTickets"].as_str(), Some("mine"));
}

#[test]
fn sibling_segments_are_exact_per_parent() {
    let meta = frontmatter(
        "metadata:\n  retiredTickets:\n    auth.1: {}\n    auth.10: {}\n    auth.1.2: {}\n    \
         auth.fix: {}\n    authx.4: {}\n",
    );
    let retired = retired_tickets(Some(&meta)).expect("well formed");
    let mut top = retired_sibling_segments(&retired, "auth", None);
    top.sort();
    assert_eq!(top, vec!["1", "10", "fix"]);
    assert_eq!(retired_sibling_segments(&retired, "auth", Some("1")), vec!["2"]);
    assert!(retired_sibling_segments(&retired, "auth", Some("10")).is_empty());
}
