// How `rhei verify` classifies what it finds, below the command: which reasons
// fail, what a reset event prunes, and the findings a hand-built store yields.

// §FS-rhei-verify.1 §FS-rhei-verify.3 §FS-rhei-evidence.10.1

mod verify_findings {
    use super::super::*;

    /// A store holding one finalized run whose manifest names one log.
    fn store(log: &[u8], recorded: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let root = rhei_core::platform::canonical_path(dir.path()).expect("root");
        fs::create_dir_all(root.join("runtime/logs")).expect("logs");
        fs::write(root.join("runtime/logs/a.log"), log).expect("log");
        let manifest = serde_json::json!({
            "run_id": "run-a",
            "status": "finished",
            "exit_code": 0,
            "started_at": "2026-10-29T00:00:00Z",
            "invocations": [],
            "files": [{ "path": "runtime/logs/a.log", "sha256": sha256_hex(recorded) }],
            "accounting": [],
            "snapshots": [],
        });
        write_evidence_manifest(&root, "run-a", &manifest).expect("manifest");
        (dir, root)
    }

    fn codes(outcome: &VerifyOutcome) -> Vec<&'static str> {
        outcome.findings.iter().map(|finding| finding.reason_code).collect()
    }

    #[test]
    fn only_disagreements_and_absences_fail() {
        for code in ["mismatch", "missing", "unlisted", "unknown_digest", "nothing_to_verify"] {
            assert!(VerifyFinding::new(code, None, None).fails(), "{code} fails");
        }
        for code in ["pruned", "snapshot_not_cached", "not_finalized"] {
            assert!(!VerifyFinding::new(code, None, None).fails(), "{code} does not fail");
        }
    }

    #[test]
    fn a_matching_store_verifies() {
        let (_dir, root) = store(b"log\n", b"log\n");
        let outcome = verify_evidence(&root, None);
        assert!(outcome.verified(), "{:?}", codes(&outcome));
        assert_eq!(outcome.runs[0].files_verified, 1);
    }

    #[test]
    fn a_changed_file_is_a_mismatch_with_both_digests() {
        let (_dir, root) = store(b"edited\n", b"log\n");
        let outcome = verify_evidence(&root, None);
        assert!(!outcome.verified());
        let finding = &outcome.findings[0];
        assert_eq!(finding.reason_code, "mismatch");
        assert_eq!(finding.recorded_sha256.as_deref(), Some(sha256_hex(b"log\n").as_str()));
        assert_eq!(finding.found_sha256.as_deref(), Some(sha256_hex(b"edited\n").as_str()));
    }

    #[test]
    fn a_deleted_file_is_missing_unless_a_reset_event_pruned_it() {
        let (_dir, root) = store(b"log\n", b"log\n");
        fs::remove_file(root.join("runtime/logs/a.log")).expect("delete");
        assert_eq!(codes(&verify_evidence(&root, None)), ["missing"]);

        let event = serde_json::json!({
            "kind": "reset",
            "runs": ["run-a"],
            "removed": ["runtime/logs/a.log"],
        });
        write_evidence_exclusively(&root.join("runtime/evidence/events/e1.json"), &event).expect("event");
        let pruned = pruned_paths(&evidence_dir(&root));
        assert!(pruned["run-a"].contains("runtime/logs/a.log"));
        let outcome = verify_evidence(&root, None);
        assert_eq!(codes(&outcome), ["pruned"]);
        assert!(outcome.verified(), "a pruned path does not fail");
    }

    #[test]
    fn a_reset_event_prunes_only_for_the_runs_it_lists() {
        let (_dir, root) = store(b"log\n", b"log\n");
        let event = serde_json::json!({ "kind": "reset", "runs": ["run-b"], "removed": ["runtime/logs/a.log"] });
        write_evidence_exclusively(&root.join("runtime/evidence/events/e1.json"), &event).expect("event");
        fs::remove_file(root.join("runtime/logs/a.log")).expect("delete");
        assert_eq!(codes(&verify_evidence(&root, None)), ["missing"]);
    }

    #[test]
    fn a_record_the_manifest_does_not_name_is_unlisted() {
        let (_dir, root) = store(b"log\n", b"log\n");
        let stray = root.join("runtime/evidence/runs/run-a/0123.start.json");
        fs::write(&stray, "{}\n").expect("stray");
        assert_eq!(codes(&verify_evidence(&root, None)), ["unlisted"]);
    }

    #[test]
    fn a_digest_selects_one_run_and_an_unknown_one_fails() {
        let (_dir, root) = store(b"log\n", b"log\n");
        let manifest = fs::read(root.join("runtime/evidence/runs/run-a/manifest.json")).expect("manifest");
        assert!(verify_evidence(&root, Some(&sha256_hex(&manifest))).verified());
        assert_eq!(codes(&verify_evidence(&root, Some(&"0".repeat(64)))), ["unknown_digest"]);
    }

    #[test]
    fn an_empty_store_has_nothing_to_verify() {
        let dir = tempfile::tempdir().expect("tmpdir");
        assert_eq!(codes(&verify_evidence(dir.path(), None)), ["nothing_to_verify"]);
    }

    #[test]
    fn a_short_digest_is_a_usage_error() {
        assert!(parse_verify_digest("abc").is_err());
        assert_eq!(parse_verify_digest(&"A".repeat(64)), Ok("a".repeat(64)));
    }
}
