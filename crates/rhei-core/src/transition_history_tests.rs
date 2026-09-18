//! Exceptional pairs remain singular and incompatible evidence refuses. §FS-rhei-complete.3.1
use super::*;

fn audit() -> ForceAudit {
    ForceAudit {
        confirmation: "typed-hop-v1".into(),
        from: "gate".into(),
        to: "work".into(),
        os_user: "operator".into(),
        reason: "fix route".into(),
        recovery_id: "018f0000-0000-7000-8000-000000000001".into(),
        result_sha256: None,
        schema_version: 1,
        task_id: "a.1".into(),
        timestamp: "2026-09-18T12:00:00Z".into(),
    }
}

#[test]
fn operator_history_pairs_coexist_with_legacy_and_unknown_metadata() {
    let (metadata, movement) = audit().pair().unwrap();
    let entries =
        parse(&format!("a.1 draft@gate\na.1 !future ignored\n{metadata}{movement}")).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].audit.is_none());
    assert_eq!(entries[1].audit, Some(audit()));
}

#[test]
fn operator_history_rejects_lone_reversed_mismatching_nonadjacent_and_duplicate_pairs() {
    let (metadata, movement) = audit().pair().unwrap();
    for raw in [
        metadata.clone(),
        format!("{movement}{metadata}"),
        format!("{metadata}a.1 gate@done\n"),
        format!("{metadata}b.1 gate@work\n"),
        format!("{metadata}a.1 !future ignored\n{movement}"),
        format!("{metadata}{movement}{metadata}{movement}"),
    ] {
        assert!(parse(&raw).is_err(), "accepted {raw}");
    }
}

#[test]
fn operator_history_rejects_noncanonical_payloads() {
    let good = String::from_utf8(canonical_json(&audit()).unwrap()).unwrap();
    for raw in [
        format!(" {good}"),
        good.replace("\"schema_version\":1", "\"schema_version\":1,\"schema_version\":1"),
        good.replace("\"reason\":\"fix route\"", "\"reason\":\" \""),
        good.replace("\"schema_version\":1", "\"schema_version\":2"),
    ] {
        assert!(
            parse(&format!("a.1 !force-v1 {}\na.1 gate@work\n", encode(raw.as_bytes()))).is_err()
        );
    }
}
