// Unit coverage for the checkpoint document and the provenance line's fields.
// §FS-rhei-summary.6 §FS-rhei-summary.2.4

fn checkpoint_task(id: &str, state: &str) -> CheckpointTask {
    CheckpointTask { id: id.to_string(), state: state.to_string() }
}

#[test]
fn a_checkpoint_prints_canonically_and_its_id_is_the_digest_of_those_bytes() {
    // §FS-rhei-summary.6.2: sorted keys and arrays, no whitespace, one newline.
    let spawn = |name: &str| CheckpointSpawn {
        task: "t".to_string(),
        state: "implement".to_string(),
        moves: 2,
        attempt: 1,
        spawn: name.to_string(),
    };
    let checkpoint = SummaryCheckpoint {
        schema: SUMMARY_CHECKPOINT_SCHEMA.to_string(),
        invocations: vec!["b".to_string(), "a".to_string()],
        spawns: vec![spawn("task-t-2.json"), spawn("task-t-1.json")],
        tasks: vec![checkpoint_task("t2", "done"), checkpoint_task("t1", "work")],
    };
    let canonical = checkpoint.canonical();
    assert_eq!(
        canonical,
        "{\"invocations\":[\"a\",\"b\"],\"schema\":\"rhei.summary.checkpoint.v1\",\
         \"spawns\":[{\"attempt\":1,\"moves\":2,\"spawn\":\"task-t-1.json\",\"state\":\"implement\",\
         \"task\":\"t\"},{\"attempt\":1,\"moves\":2,\"spawn\":\"task-t-2.json\",\"state\":\"implement\",\
         \"task\":\"t\"}],\"tasks\":[{\"id\":\"t1\",\"state\":\"work\"},{\"id\":\"t2\",\"state\":\"done\"}]}\n"
    );
    assert_eq!(checkpoint.id(), sha256_hex(canonical.as_bytes())[..12]);

    // A replay re-encodes what it read, so the order it was read in is moot.
    let read: SummaryCheckpoint = serde_json::from_str(&canonical).expect("round trip");
    assert_eq!(read.id(), checkpoint.id());
}

#[test]
fn a_checkpoint_refuses_an_unknown_member() {
    // §FS-rhei-summary.6: the schema closes the object.
    let text = r#"{"schema":"rhei.summary.checkpoint.v1","invocations":[],"spawns":[],"tasks":[],"path":"/x"}"#;
    assert!(serde_json::from_str::<SummaryCheckpoint>(text).is_err());
}

#[test]
fn the_laid_fence_reads_as_the_provenance_lines_template() {
    // §FS-rhei-summary.2.4 §FS-rhei-library.7.1
    let src = "ab".repeat(32);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let inputs = format!("inputs:sha256:{}", "cd".repeat(32));
    let fence = |fields: &str| format!("# --- laid tiny 1.0.0 src:sha256:{src} {fields} {inputs} ---");
    assert_eq!(
        laid_fence_provenance(&fence(&format!("git:example-org/templates@{sha} clean"))).as_deref(),
        Some("`tiny` 1.0.0 @ example-org/templates@0123456 (clean)")
    );
    assert_eq!(
        laid_fence_provenance(&fence(&format!("git:{sha} dirty"))).as_deref(),
        Some("`tiny` 1.0.0 @ 0123456 (dirty)")
    );
    assert_eq!(
        laid_fence_provenance(&fence("unversioned")).as_deref(),
        Some("`tiny` 1.0.0 src:sha256:abababababab (unversioned)")
    );
    assert_eq!(
        laid_fence_provenance(&fence("builtin")).as_deref(),
        Some("`tiny` 1.0.0 (built into rhei)")
    );
    // An inclusion fence is not the lay's.
    assert_eq!(laid_fence_provenance(&format!("# --- tiny 1.0.0 src:sha256:{src} clean ---")), None);
}

#[test]
fn a_build_reads_as_version_at_commit_and_unknown_where_none_was_recorded() {
    // §FS-rhei-summary.2.4
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let build = |commit: Option<&str>, dirty: Option<bool>| rhei_tui::RheiBuild {
        version: "0.6.4".to_string(),
        commit: commit.map(str::to_string),
        dirty,
    };
    assert_eq!(rhei_build_label(Some(&build(Some(sha), Some(false)))), "0.6.4 @ 0123456");
    assert_eq!(rhei_build_label(Some(&build(Some(sha), Some(true)))), "0.6.4 @ 0123456 (dirty)");
    assert_eq!(rhei_build_label(Some(&build(None, None))), "0.6.4 @ unknown");
    assert_eq!(rhei_build_label(None), "unknown");
}

#[test]
fn an_empty_reading_names_only_the_template() {
    // §FS-rhei-summary.5
    let inspection = CostInspection::from_records(None, Vec::new());
    assert_eq!(
        summary_provenance_line(&inspection, "000000000000", "unknown"),
        "Provenance: template unknown · no invocations recorded"
    );
}
