struct ClaimFixture {
    dir: tempfile::TempDir,
    metadata: PathBuf,
    task: PathBuf,
}

impl ClaimFixture {
    const ORIGINAL_METADATA: &'static str = "metadata-before\n";
    const ORIGINAL_TASK: &'static str = "### Task 1: Work\n**State:** draft\n";
    const UPDATED_METADATA: &'static str = "metadata-after-with-counted-visit\n";
    const UPDATED_TASK: &'static str = "### Task 1: Work\n**State:** pending\n";

    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let metadata = dir.path().join("index.rhei.md");
        let task = dir.path().join("task.md");
        fs::write(&metadata, Self::ORIGINAL_METADATA).expect("metadata");
        fs::write(&task, Self::ORIGINAL_TASK).expect("task");
        Self { dir, metadata, task }
    }

    fn files(&self) -> TransitionFiles<'_> {
        TransitionFiles {
            task_file: &self.task,
            metadata_file: &self.metadata,
            metadata_id: "1",
            artifact_root: self.dir.path(),
            artifact_id: "plan.1",
        }
    }

    fn assert_original(&self) {
        assert_eq!(fs::read_to_string(&self.metadata).unwrap(), Self::ORIGINAL_METADATA);
        assert_eq!(fs::read_to_string(&self.task).unwrap(), Self::ORIGINAL_TASK);
        assert_eq!(
            fs::read_to_string(self.dir.path().join("runtime/state-transitions.log"))
                .unwrap_or_default(),
            ""
        );
    }
}

fn begin_claim<'a>(
    fixture: &'a ClaimFixture,
    metadata_lock: &'a LockedPlanFile,
    task_lock: &'a LockedPlanFile,
) -> ClaimTransaction<'a> {
    ClaimTransaction::begin(
        fixture.files(),
        metadata_lock,
        Some(task_lock),
        ClaimFixture::ORIGINAL_METADATA,
        ClaimFixture::ORIGINAL_TASK,
    )
    .expect("begin claim")
}

/// A state or metadata write error restores both files and counted metadata.
/// §FS-rhei-next.3.1
#[test]
fn claim_transaction_state_write_failure_restores_task_and_metadata() {
    let fixture = ClaimFixture::new();
    let metadata_lock = LockedPlanFile::open(&fixture.metadata).unwrap();
    let task_lock = LockedPlanFile::open(&fixture.task).unwrap();
    let mut transaction = begin_claim(&fixture, &metadata_lock, &task_lock);
    set_claim_faults(vec![(ClaimFaultPoint::StateTaskWrite, "task disk full")]);

    let error = transaction
        .write_state(ClaimFixture::UPDATED_METADATA, Some(ClaimFixture::UPDATED_TASK))
        .expect_err("state task write");
    let restored = transaction.rollback(error);
    drop(transaction);
    drop(task_lock);
    drop(metadata_lock);

    assert!(restored.error.to_string().contains("task persistence injection"));
    fixture.assert_original();
}

/// A metadata persistence error occurs before the task rewrite and preserves
/// the source bytes on both sides. §FS-rhei-next.3.1
#[test]
fn claim_transaction_metadata_write_failure_changes_nothing() {
    let fixture = ClaimFixture::new();
    let metadata_lock = LockedPlanFile::open(&fixture.metadata).unwrap();
    let task_lock = LockedPlanFile::open(&fixture.task).unwrap();
    let mut transaction = begin_claim(&fixture, &metadata_lock, &task_lock);
    set_claim_faults(vec![(ClaimFaultPoint::StateMetadataWrite, "metadata disk full")]);

    let error = transaction
        .write_state(ClaimFixture::UPDATED_METADATA, Some(ClaimFixture::UPDATED_TASK))
        .expect_err("metadata write");
    transaction.rollback(error);
    drop(transaction);
    drop(task_lock);
    drop(metadata_lock);

    fixture.assert_original();
}

/// Ownership is still pre-commit: an assignee write error restores state,
/// ownership, counted metadata, and ledger bytes. §FS-rhei-next.3.1
#[test]
fn claim_transaction_assignee_failure_restores_every_persistence_surface() {
    let fixture = ClaimFixture::new();
    let metadata_lock = LockedPlanFile::open(&fixture.metadata).unwrap();
    let task_lock = LockedPlanFile::open(&fixture.task).unwrap();
    let mut transaction = begin_claim(&fixture, &metadata_lock, &task_lock);
    transaction
        .write_state(ClaimFixture::UPDATED_METADATA, Some(ClaimFixture::UPDATED_TASK))
        .unwrap();
    set_claim_faults(vec![(ClaimFaultPoint::AssigneeWrite, "rename refused")]);

    let error = transaction
        .write_assignee(
            ClaimFixture::UPDATED_METADATA,
            Some(ClaimFixture::UPDATED_TASK),
            "1",
            "codex",
        )
        .expect_err("assignee write");
    transaction.rollback(error);
    drop(transaction);
    drop(task_lock);
    drop(metadata_lock);

    fixture.assert_original();
}

/// Ledger preflight precedes every plan write. §FS-rhei-next.3.1
#[test]
fn claim_transaction_ledger_preflight_failure_changes_nothing() {
    let fixture = ClaimFixture::new();
    let metadata_lock = LockedPlanFile::open(&fixture.metadata).unwrap();
    let task_lock = LockedPlanFile::open(&fixture.task).unwrap();
    set_claim_faults(vec![(ClaimFaultPoint::LedgerPreflight, "runtime unavailable")]);

    let error = ClaimTransaction::begin(
        fixture.files(),
        &metadata_lock,
        Some(&task_lock),
        ClaimFixture::ORIGINAL_METADATA,
        ClaimFixture::ORIGINAL_TASK,
    )
    .err()
    .expect("ledger preflight");
    drop(task_lock);
    drop(metadata_lock);

    assert!(error.to_string().contains("ledger preflight injection"));
    fixture.assert_original();
}

/// A partial claim append is reversed while another writer is excluded; that
/// writer's later successful line survives intact. §FS-rhei-next.3.1
#[test]
fn claim_transaction_partial_ledger_failure_preserves_another_writer() {
    let fixture = ClaimFixture::new();
    let runtime = fixture.dir.path().join("runtime");
    fs::create_dir_all(&runtime).unwrap();
    let ledger = runtime.join("state-transitions.log");
    fs::write(&ledger, "plan.9 pending@completed\n").unwrap();
    let metadata_lock = LockedPlanFile::open(&fixture.metadata).unwrap();
    let task_lock = LockedPlanFile::open(&fixture.task).unwrap();
    let mut transaction = begin_claim(&fixture, &metadata_lock, &task_lock);
    transaction
        .write_state(ClaimFixture::UPDATED_METADATA, Some(ClaimFixture::UPDATED_TASK))
        .unwrap();
    transaction
        .write_assignee(
            ClaimFixture::UPDATED_METADATA,
            Some(ClaimFixture::UPDATED_TASK),
            "1",
            "manual",
        )
        .unwrap();

    let root = fixture.dir.path().to_path_buf();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let writer_barrier = barrier.clone();
    let writer = std::thread::spawn(move || {
        writer_barrier.wait();
        append_state_transition_log_entry(&root, "plan.2", "review", "completed")
    });
    barrier.wait();
    set_claim_faults(vec![(ClaimFaultPoint::LedgerAppend, "short write")]);
    let error = transaction.commit("draft", "pending").expect_err("partial append");
    transaction.rollback(error);
    drop(transaction);
    drop(task_lock);
    drop(metadata_lock);
    writer.join().expect("writer thread").expect("writer append");

    assert_eq!(
        fs::read_to_string(&fixture.metadata).unwrap(),
        ClaimFixture::ORIGINAL_METADATA
    );
    assert_eq!(fs::read_to_string(&fixture.task).unwrap(), ClaimFixture::ORIGINAL_TASK);
    assert_eq!(
        fs::read_to_string(ledger).unwrap(),
        "plan.9 pending@completed\nplan.2 review@completed\n"
    );
}

/// A restoration double fault identifies both failures and all three paths,
/// without making the ordinary retryability promise. §FS-rhei-next.3.1
#[test]
fn claim_transaction_restoration_double_fault_reports_errors_and_paths() {
    let fixture = ClaimFixture::new();
    let metadata_lock = LockedPlanFile::open(&fixture.metadata).unwrap();
    let task_lock = LockedPlanFile::open(&fixture.task).unwrap();
    let mut transaction = begin_claim(&fixture, &metadata_lock, &task_lock);
    transaction
        .write_state(ClaimFixture::UPDATED_METADATA, Some(ClaimFixture::UPDATED_TASK))
        .unwrap();
    set_claim_faults(vec![
        (ClaimFaultPoint::LedgerAppend, "originating short write"),
        (ClaimFaultPoint::RestoreMetadata, "metadata restore denied"),
        (ClaimFaultPoint::RestoreTask, "task restore denied"),
        (ClaimFaultPoint::RestoreLedger, "ledger restore denied"),
    ]);

    let original = transaction.commit("draft", "pending").expect_err("append");
    let rollback = transaction.rollback(original);
    let error = rollback.error.to_string();
    drop(transaction);
    drop(task_lock);
    drop(metadata_lock);

    assert!(!rollback.restored);
    assert!(error.contains("originating short write"));
    assert!(error.contains("metadata restore denied"));
    assert!(error.contains("task restore denied"));
    assert!(error.contains("ledger restore denied"));
    assert!(error.contains(&fixture.task.display().to_string()));
    assert!(error.contains(&fixture.metadata.display().to_string()));
    assert!(error.contains("runtime/state-transitions.log"));
    assert!(!error.contains("retryable"));
}
