// Boundary coverage for attribution and summaries without own records.
// §FS-rhei-validate.4.7
mod snapshot_validation_warning_tests {
    use super::*;

    fn orphan(task_id: &str, visit: u64) -> SnapshotRecord {
        SnapshotRecord {
            path: PathBuf::new(),
            manifest: serde_json::Value::Null,
            task_id: task_id.to_string(),
            snapshot_name: "_state".to_string(),
            emitting_state: "old-state".to_string(),
            visit,
            target_slug: "old-target".to_string(),
            generation: 1,
            created_at: String::new(),
            transcript_bytes: 0,
            completion: "success".to_string(),
            produced_by: "orchestrator".to_string(),
            is_current: true,
        }
    }

    #[test]
    fn unknown_snapshot_owners_keep_detail_beside_known_member_summary() {
        let records = [
            orphan("retired-rhei.deleted", 1),
            orphan("other.deleted", 1),
            orphan("unqualified", 1),
            orphan("other.", 1),
            orphan("own.deleted", 1),
        ];
        let ids = ["own".to_string(), "other".to_string()];
        let warnings = format_snapshot_orphan_warnings(&records, &ids, Some("own"), "inspect");
        assert_eq!(warnings.len(), 5);
        for record in [&records[0], &records[2], &records[3], &records[4]] {
            assert!(warnings.iter().any(|warning| warning.contains(&record.display_ref())));
        }
        assert!(!warnings.iter().any(|warning| warning.contains(&records[1].display_ref())));
        assert!(warnings.last().unwrap().contains("1 orphaned snapshots"));
        assert!(warnings.last().unwrap().contains("other: 1"));
    }

    #[test]
    fn other_member_snapshots_need_no_own_orphans_to_produce_sorted_record_counts() {
        let records = [orphan("z.deleted", 1), orphan("a.deleted", 1), orphan("a.deleted", 2)];
        let ids = ["own".to_string(), "z".to_string(), "a".to_string()];
        let warnings = format_snapshot_orphan_warnings(&records, &ids, Some("own"), "inspect");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("3 orphaned snapshots"));
        assert!(warnings[0].contains("a: 2, z: 1"));
        assert!(warnings[0].contains("`inspect`"));
    }

    #[test]
    fn snapshot_inspection_hint_places_machine_override_on_parent_command() {
        let hint = snapshot_orphan_inspection_command(Path::new("project"), Some(Path::new("custom.yaml")));
        // Simple shell-safe words let the real parser check flag placement on
        // every platform; the CLI regressions cover quoted project paths.
        let parsed = Cli::try_parse_from(hint.split_whitespace()).expect("executable hint");
        let Commands::Snapshot {
            state_machine,
            command: SnapshotCommand::List { plan, orphaned, produced_by, .. },
        } = parsed.command else {
            panic!("expected a snapshot list command: {hint}");
        };
        assert_eq!(state_machine.as_deref(), Some(Path::new("custom.yaml")));
        assert_eq!(plan, Path::new("project"));
        assert!(orphaned);
        assert!(matches!(produced_by, SnapshotProducedByFilter::All));
    }
}
