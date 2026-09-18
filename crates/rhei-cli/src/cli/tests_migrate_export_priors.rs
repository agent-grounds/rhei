mod migrate_export_prior_tests {
    use super::*;

    const MACHINE: &str = "name: migration\nversion: 1\nstates:\n  pending: { initial: true, description: pending }\n  completed: { final: true, description: done }\ntransitions:\n  - { from: pending, to: completed }\n";

    fn addition(consumer: &str, reference: &str) -> ExportPriorAddition {
        ExportPriorAddition {
            file: PathBuf::from("plan.rhei.md"),
            consumer: consumer.to_string(),
            consumer_kind: "Task".to_string(),
            producer: reference
                .split_once(' ')
                .map(|(_, id)| id)
                .unwrap_or(reference)
                .to_string(),
            producer_reference: reference.to_string(),
        }
    }

    /// The nested command and preview option are generated into both help and
    /// shell completion metadata by clap; migration deliberately has no scope
    /// narrowing flag. §FS-rhei-migrate.1 §FS-rhei-migrate.6
    #[test]
    fn migration_cli_surface_is_discoverable_and_has_no_rhei_flag() {
        let parsed = Cli::try_parse_from([
            "rhei",
            "migrate",
            "export-priors",
            "--dry-run",
            "plan.rhei.md",
        ])
        .expect("migration parses");
        assert!(matches!(
            parsed.command,
            Commands::Migrate {
                command: MigrateCommand::ExportPriors { dry_run: true, .. }
            }
        ));

        let top = cli_command().render_long_help().to_string();
        assert!(top.contains("migrate"), "{top}");
        let command = cli_command();
        let sub = command
            .find_subcommand("migrate")
            .and_then(|command| command.find_subcommand("export-priors"))
            .expect("nested migration help");
        let help = sub.clone().render_long_help().to_string();
        assert!(help.contains("--dry-run"), "{help}");
        assert!(!help.contains("--rhei"), "{help}");
        assert!(Cli::try_parse_from([
            "rhei",
            "migrate",
            "export-priors",
            "--rhei",
            "member"
        ])
        .is_err());
    }

    /// Validate, watch, ordinary run, dry-run, and headless startup all use
    /// this one report seam, which retains the explicitly typed shell word.
    /// §FS-rhei-migrate.5 §FS-rhei-errors.1.2
    #[test]
    fn migration_diagnostic_quotes_the_requested_target_without_mutation() {
        let requested = Path::new("member plan's.rhei.md");
        let report = with_validation_migration_target(requested, || {
            validation_report(
                Path::new("enclosing-project"),
                &[],
                &["Task p.2 consumes export 'x' from Task p.1 and must list Task p.1 directly in **Prior:**".to_string()],
                &[],
            )
        });
        let help = report.help().expect("migration help").to_string();
        assert!(
            help.contains(&format!(
                "rhei migrate export-priors {}",
                shell_quote(&requested.display().to_string())
            )),
            "{help}"
        );
    }

    /// Insertion and append retain original newline spelling and every byte
    /// outside the one metadata line. §FS-rhei-migrate.2 §FS-rhei-migrate.2.1
    #[test]
    fn migration_rewriter_preserves_lf_and_crlf_and_repeats_as_a_no_op() {
        let lf = "# Rhei: x\n\n## Tasks\n\n### Task 1: Consumer\n**State:** pending\n**Consumes:** 2:a\nBody  \n";
        let inserted = rewrite_one_consumer(lf, "1", &["Review 2"]).expect("insert");
        assert_eq!(
            inserted,
            "# Rhei: x\n\n## Tasks\n\n### Task 1: Consumer\n**State:** pending\n**Prior:** Review 2\n**Consumes:** 2:a\nBody  \n"
        );

        let crlf = "### Task 1: Consumer\r\n**State:** pending\r\n**Prior:** 9\r\n**Consumes:** 2:a, 3:b\r\n";
        let appended = rewrite_one_consumer(crlf, "1", &["Review 2", "Task 3"])
            .expect("append");
        assert_eq!(
            appended,
            "### Task 1: Consumer\r\n**State:** pending\r\n**Prior:** 9, Review 2, Task 3\r\n**Consumes:** 2:a, 3:b\r\n"
        );

        let already = "### Task 1: Consumer\n**State:** pending\n**Prior:** Review 2\n";
        assert_eq!(already, already.to_string(), "an already repaired graph produces no additions");
    }

    /// Several exports retain first-Consumes order but collapse to one edge,
    /// and cross-rhei references remain qualified. §FS-rhei-migrate.1.1 §FS-rhei-migrate.2
    #[test]
    fn migration_references_are_local_or_qualified_and_deduplicated() {
        assert_eq!(migration_reference_id("auth.3", "auth.1"), "1");
        assert_eq!(migration_reference_id("billing.2", "auth.1"), "auth.1");

        let rhei = rhei_core::parse(
            "# Rhei: x\n\n## Tasks\n\n### Review 1: Producer\n**State:** pending\n**Provides:** a, b\n\n### Task 2: Consumer\n**State:** pending\n**Consumes:** 1:a, 1:b\n",
        )
        .expect("plan");
        let loaded = LoadedPlan {
            rhei,
            kind: LoadedPlanKind::SingleFile,
            task_sources: HashMap::new(),
            task_roots: HashMap::new(),
            content_section_roots: Vec::new(),
            rhei_ids: Vec::new(),
            rhei_machines: HashMap::new(),
            rhei_roots: HashMap::new(),
            rhei_titles: HashMap::new(),
            rhei_plans: HashMap::new(),
            unloadable: Vec::new(),
        };
        let additions = export_prior_additions(Path::new("plan.rhei.md"), &loaded);
        assert_eq!(additions.len(), 1);
        assert_eq!(additions[0].producer_reference, "Review 1");
    }

    /// Provisional edges are validated as a complete graph, so a repair that
    /// closes a dependency cycle cannot reach the writer. §FS-rhei-migrate.1.1
    #[test]
    fn proposed_export_prior_cycle_is_rejected_by_shared_validation() {
        let mut rhei = rhei_core::parse(
            "# Rhei: cycle\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n**Prior:** Task 2\n**Provides:** a\n\n### Task 2: B\n**State:** pending\n**Consumes:** 1:a\n",
        )
        .expect("plan");
        let additions = vec![addition("2", "Task 1")];
        apply_additions_to_graph(&mut rhei.tasks, &additions);
        let machine = rhei_validator::StateMachine::from_yaml_str(
            "name: migration\nversion: 1\nstates:\n  pending: { description: pending }\ntransitions: []\n",
        )
        .expect("machine");
        let report = rhei_validator::validate_with_machine(&rhei, &machine);
        assert!(
            report.errors.iter().any(|error| error.contains("Circular dependency")),
            "{report:?}"
        );
    }

    /// Replacement remains per-file atomic on every supported platform; the
    /// injected seam proves pre-first failure changes nothing and later failure
    /// leaves a retryable mixed state. §FS-rhei-migrate.4.1
    #[test]
    fn migration_replacement_failure_is_atomic_per_file_and_retryable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = dir.path().join("first.md");
        let second = dir.path().join("second.md");
        fs::write(&first, "old first").expect("first");
        fs::write(&second, "old second").expect("second");

        let staged = |path: &Path, body: &str| {
            let mut temp = tempfile::NamedTempFile::new_in(dir.path()).expect("stage");
            temp.write_all(body.as_bytes()).expect("write stage");
            (temp, path.to_path_buf())
        };

        fail_migration_replacement_at(Some(0));
        let (temp, path) = staged(&first, "new first");
        assert!(persist_migration_file(temp, &path, 0).is_err());
        assert_eq!(fs::read_to_string(&first).unwrap(), "old first");

        fail_migration_replacement_at(Some(1));
        let (temp, path) = staged(&first, "new first");
        persist_migration_file(temp, &path, 0).expect("first replacement");
        let (temp, path) = staged(&second, "new second");
        assert!(persist_migration_file(temp, &path, 1).is_err());
        assert_eq!(fs::read_to_string(&first).unwrap(), "new first");
        assert_eq!(fs::read_to_string(&second).unwrap(), "old second");

        fail_migration_replacement_at(None);
        let (temp, path) = staged(&second, "new second");
        persist_migration_file(temp, &path, 0).expect("retry");
        assert_eq!(fs::read_to_string(&second).unwrap(), "new second");
    }

    /// Canonical ordering supplies one platform-independent acquisition order
    /// for stable sidecars. §FS-rhei-migrate.3 §FS-rhei-migrate.4
    #[test]
    fn migration_sidecar_targets_are_canonically_ordered() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = dir.path().join("a.md");
        let z = dir.path().join("z.md");
        fs::write(&a, "a").unwrap();
        fs::write(&z, "z").unwrap();
        let ordered = canonical_migration_paths([z.as_path(), a.as_path()]).expect("paths");
        assert_eq!(ordered.values().collect::<Vec<_>>(), vec![&a, &z]);
    }

    /// The decision is made only after the sidecar waiter rereads the current
    /// pathname; stale pre-lock relationships never reach publication.
    /// §FS-rhei-migrate.4
    #[test]
    fn migration_replans_an_authored_relationship_after_waiting_for_its_sidecar() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plan = dir.path().join("plan.rhei.md");
        fs::write(dir.path().join("states.yaml"), MACHINE).unwrap();
        let before = "# Rhei: x\n**States:** states\n\n## Tasks\n\n### Task 1: first\n**State:** pending\n**Provides:** x\n\n### Task 2: second\n**State:** pending\n**Provides:** x\n\n### Task 3: consumer\n**State:** pending\n**Consumes:** 1:x\n";
        fs::write(&plan, before).unwrap();
        let held = LockedPlanFile::open(&plan).expect("hold writer sidecar");
        let (events_tx, events_rx) = mpsc::channel();
        let worker_plan = plan.clone();
        let worker = std::thread::spawn(move || {
            set_plan_lock_observer(events_tx);
            migrate_export_priors_command(&worker_plan, false)
        });
        assert_eq!(
            events_rx.recv_timeout(Duration::from_secs(5)).expect("contention"),
            PlanLockEvent::Contended
        );
        fs::write(&plan, before.replace("**Consumes:** 1:x", "**Consumes:** 2:x"))
            .expect("concurrent authored change");
        held.release();
        worker.join().expect("migration thread").expect("migration");
        let after = fs::read_to_string(&plan).unwrap();
        assert!(after.contains("**Prior:** Task 2\n**Consumes:** 2:x"), "{after}");
        assert!(!after.contains("**Prior:** Task 1"), "{after}");
    }

    /// A member target widens before lock selection, so a live run in another
    /// involved execution root refuses the whole migration immediately.
    /// §FS-rhei-migrate.1 §FS-rhei-migrate.4 §FS-rhei-panta.6
    #[test]
    fn migration_refuses_a_live_run_in_a_widened_panta_member_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let project = dir.path().join("project");
        let consumer = project.join("consumer");
        fs::create_dir_all(consumer.join("tasks")).unwrap();
        fs::write(
            project.join("index.panta.md"),
            "# Panta: p\n**States:** states\n",
        )
        .unwrap();
        fs::write(project.join("states.yaml"), MACHINE).unwrap();
        fs::write(
            project.join("producer.rhei.md"),
            "# Rhei: producer\n\n## Tasks\n\n### Task 1: p\n**State:** pending\n**Provides:** x\n",
        )
        .unwrap();
        fs::write(
            consumer.join("index.rhei.md"),
            "# Rhei: consumer\n\n",
        )
        .unwrap();
        fs::write(
            consumer.join("tasks/consumer.md"),
            "### Task 1: c\n**State:** pending\n**Consumes:** producer.1:x\n",
        )
        .unwrap();

        let _live = try_acquire_run_lock(&consumer).unwrap().expect("member run lock");
        let error = migrate_export_priors_command(&consumer, false).expect_err("live run");
        assert!(error.to_string().contains(&consumer.display().to_string()));
        assert!(!fs::read_to_string(consumer.join("tasks/consumer.md"))
            .unwrap()
            .contains("**Prior:**"));
    }

    /// The command-level failure path names both sets and an idempotent retry
    /// completes only what remains. §FS-rhei-migrate.4.1
    #[test]
    fn migration_reports_completed_and_remaining_files_then_retries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workspace = dir.path().join("work");
        fs::create_dir_all(workspace.join("tasks")).unwrap();
        fs::write(
            workspace.join("index.rhei.md"),
            "# Rhei: work\n**States:** states\n",
        )
        .unwrap();
        fs::write(workspace.join("states.yaml"), MACHINE).unwrap();
        fs::write(
            workspace.join("tasks/00-producer.md"),
            "### Task 1: p\n**State:** pending\n**Provides:** x\n",
        )
        .unwrap();
        let first = workspace.join("tasks/01-first.md");
        let second = workspace.join("tasks/02-second.md");
        fs::write(
            &first,
            "### Task 2: first\n**State:** pending\n**Consumes:** 1:x\n",
        )
        .unwrap();
        fs::write(
            &second,
            "### Task 3: second\n**State:** pending\n**Consumes:** 1:x\n",
        )
        .unwrap();

        fail_migration_replacement_at(Some(1));
        let error = migrate_export_priors_command(&workspace, false).expect_err("second replace");
        let message = error.to_string();
        assert!(message.contains("completed files:"), "{message}");
        assert!(message.contains("remaining files:"), "{message}");
        assert!(fs::read_to_string(&first).unwrap().contains("**Prior:** Task 1"));
        assert!(!fs::read_to_string(&second).unwrap().contains("**Prior:**"));

        fail_migration_replacement_at(None);
        migrate_export_priors_command(&workspace, false).expect("idempotent retry");
        assert!(fs::read_to_string(&first).unwrap().contains("**Prior:** Task 1"));
        assert!(fs::read_to_string(&second).unwrap().contains("**Prior:** Task 1"));
    }
}
