// The evidence store's small rules, below `rhei run`: the record key, the
// program id's spelling, env reduction, the version probe's command and cache,
// the manifest's bytes, and the published schema list.

// §FS-rhei-evidence.2.1 §FS-rhei-evidence.6.1 §FS-rhei-evidence.7 §FS-rhei-evidence.9

mod evidence_store {
    use super::super::*;

    fn run_in(root: &Path) -> EvidenceRun {
        EvidenceRun::new(
            root,
            "run-1",
            "2026-10-29T00:00:00Z",
            &rhei_tui::RunProvenance::default(),
            RheiSettings::default(),
        )
    }

    #[test]
    fn the_key_is_the_first_32_hex_of_the_ids_sha256() {
        let id = "plan.1::work::mock::visit-1::run-a::move-0::attempt-1";
        let key = evidence_key(id);
        assert_eq!(key.len(), 32);
        assert_eq!(key, sha256_hex(id.as_bytes())[..32]);
        assert!(!key.contains("::"), "a raw id is never a file name");
    }

    #[test]
    fn a_program_id_takes_the_agent_spelling_with_program_as_its_target() {
        assert_eq!(
            program_invocation_id("plan.1", "measure", 2, "run-a", 3, 1),
            "plan.1::measure::program::visit-2::run-run-a::move-3::attempt-1"
        );
    }

    #[test]
    fn every_env_map_at_any_depth_becomes_its_sorted_names() {
        let mut config = serde_json::json!({
            "program": { "env": { "TOKEN": "s3cret", "A": "1" } },
            "mcp_servers": [{ "id": "pg", "definition": { "env": { "PGPASS": "x" } } }],
            "env_like": "kept",
        });
        evidence_env_names(&mut config);
        assert_eq!(config["program"]["env"], serde_json::json!(["A", "TOKEN"]));
        assert_eq!(config["mcp_servers"][0]["definition"]["env"], serde_json::json!(["PGPASS"]));
        assert_eq!(config["env_like"], "kept");
        assert!(!config.to_string().contains("s3cret"), "no value survives");
    }

    #[test]
    fn the_configuration_is_retained_as_one_canonical_document() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let run = run_in(dir.path());
        let a = evidence_config(&run, serde_json::json!({ "b": 1, "a": { "env": { "K": "v" } } }));
        let b = evidence_config(&run, serde_json::json!({ "a": { "env": { "K": "w" } }, "b": 1 }));
        assert_eq!(a, b, "key order and env values do not change the document");
        assert_eq!(a["stored"], "original");
        let hex = a["sha256"].as_str().expect("sha256");
        let stored = fs::read(evidence_dir(&run.root).join("objects/sha256").join(hex)).expect("object");
        assert_eq!(stored, b"{\"a\":{\"env\":[\"K\"]},\"b\":1}\n");
    }

    #[test]
    fn version_args_are_appended_to_the_whole_command() {
        let command = vec!["python3".to_string(), "stub.py".to_string(), "exec".to_string()];
        assert_eq!(
            version_probe_argv(&command, &["--version".to_string()]),
            ["python3", "stub.py", "exec", "--version"]
        );
    }

    #[test]
    fn the_probe_cache_holds_until_the_executables_size_or_mtime_changes() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let run = run_in(dir.path());
        let argv = vec!["cli".to_string(), "--version".to_string()];
        let mtime = std::time::SystemTime::UNIX_EPOCH;
        let calls = std::cell::Cell::new(0);
        let probe = |_: &[String]| {
            calls.set(calls.get() + 1);
            (Some(format!("cli {}", calls.get())), None)
        };
        let first = run.cached_version_probe(argv.clone(), Some((10, Some(mtime))), probe);
        let same = run.cached_version_probe(argv.clone(), Some((10, Some(mtime))), probe);
        assert_eq!((first.0.as_deref(), same.0.as_deref()), (Some("cli 1"), Some("cli 1")));
        let resized = run.cached_version_probe(argv.clone(), Some((11, Some(mtime))), probe);
        assert_eq!(resized.0.as_deref(), Some("cli 2"), "a new size probes again");
        let later = mtime + Duration::from_secs(1);
        let touched = run.cached_version_probe(argv, Some((11, Some(later))), probe);
        assert_eq!(touched.0.as_deref(), Some("cli 3"), "a new mtime probes again");
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn the_version_is_the_first_non_empty_stdout_line_trimmed() {
        assert_eq!(version_probe_line(b"\n  stub-cli 1.2.3  \nmore\n").as_deref(), Some("stub-cli 1.2.3"));
        assert_eq!(version_probe_line(b"\n \n"), None);
    }

    #[test]
    fn the_manifest_is_canonical_text_and_its_digest_is_the_files() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let root = rhei_core::platform::canonical_path(dir.path()).expect("root");
        let manifest = serde_json::json!({ "z": [1, 2], "a": "run" });
        let written = write_evidence_manifest(&root, "run-1", &manifest).expect("written");
        let bytes = fs::read(root.join("runtime/evidence/runs/run-1/manifest.json")).expect("file");
        assert_eq!(bytes, b"{\"a\":\"run\",\"z\":[1,2]}\n");
        assert_eq!(written["path"], "runtime/evidence/runs/run-1/manifest.json");
        assert_eq!(written["sha256"], sha256_hex(&bytes));
        assert!(write_evidence_manifest(&root, "run-1", &manifest).is_none(), "written once");
    }

    #[test]
    fn a_reset_event_lists_its_scope_runs_and_removals() {
        let event = reset_event_value(
            "e1",
            vec!["auth".to_string()],
            vec!["auth.1".to_string()],
            vec!["run-a".to_string()],
            vec!["runtime/logs/x.log".to_string()],
        );
        assert_eq!(event["schema"], "rhei.evidence.event.v1");
        assert_eq!(event["kind"], "reset");
        assert_eq!(event["runs"], serde_json::json!(["run-a"]));
        assert_eq!(event["removed"], serde_json::json!(["runtime/logs/x.log"]));
    }

    #[test]
    fn the_evidence_and_checkpoint_schemas_are_published() {
        let published: Vec<&str> = PUBLISHED_ACCOUNTING_SCHEMAS.iter().map(|(id, _)| *id).collect();
        for id in [
            "rhei.evidence.event.v1",
            "rhei.evidence.invocation.v1",
            "rhei.evidence.manifest.v1",
            "rhei.summary.checkpoint.v1",
        ] {
            assert!(published.contains(&id), "{id} is published: {published:?}");
        }
    }
}
