// `rhei remove`'s pieces in isolation, and its interruption at every boundary
// the writer reaches. §FS-rhei-remove

const REMOVE_MACHINE: &str = "name: remove\nversion: 1\nstates:\n  pending:\n    initial: true\n  \
    completed:\n    final: true\n  cancelled:\n    final: true\ntransitions:\n  - {from: pending, to: completed}\n  \
    - {from: pending, to: cancelled}\n";

const REMOVE_PLAN: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: Keep\n**State:** pending\n\n\
    ### Task 2: Mistake\n**State:** pending\n";

fn remove_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    fs::write(&plan, REMOVE_PLAN).unwrap();
    fs::write(&machine, REMOVE_MACHINE).unwrap();
    fs::create_dir_all(dir.path().join("runtime/exports/plan.2")).unwrap();
    (dir, plan, machine)
}

fn interrupt_removal_at(boundary: &str) {
    let boundary = boundary.to_string();
    FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
        if point == boundary { Err(miette!("interrupted at {point}")) } else { Ok(()) }
    })));
}

fn clear_removal_interrupt() { FORCED_BOUNDARY.with(|hook| hook.borrow_mut().take()); }

fn remove_ticket(plan: &Path, machine: &Path, ticket: &str) -> MietteResult<()> {
    remove_command(Some(plan.to_path_buf()), Some(ticket.to_string()), &[], false, Some(machine))
}

/// An interrupted removal leaves its marker, every other entry point refuses
/// and names the command that finishes it, and only `rhei remove` of the same
/// ticket resumes it — to the same end state an uninterrupted one reaches.
/// §FS-rhei-remove.6.2
#[test]
fn an_interrupted_removal_resumes_only_through_remove() {
    for point in ["removal-image-0", "removal-validate", "removal-residue", "removal-clear"] {
        let (dir, plan, machine) = remove_fixture();
        let marker = dir.path().join(rhei_core::root_access::REMOVAL_MARKER);
        interrupt_removal_at(point);
        let error = remove_ticket(&plan, &machine, "2").unwrap_err();
        clear_removal_interrupt();
        assert!(error.to_string().contains(point), "{point}: {error}");
        assert!(marker.is_file(), "{point}: the marker survives the interruption");

        // Every other entry point refuses while the marker stands.
        let refused = load_plan(&plan).err().map(|err| err.to_string()).unwrap_or_default();
        assert!(refused.contains("removal pending: plan.2"), "{point}: load_plan said {refused:?}");
        assert!(refused.contains("rhei remove plan.2"), "{point}: names the command: {refused:?}");
        let other = remove_ticket(&plan, &machine, "1").unwrap_err().to_string();
        assert!(other.contains("removal pending: plan.2"), "{point}: another removal: {other}");
        let dry = remove_command(Some(plan.clone()), Some("2".into()), &[], true, Some(&machine));
        assert!(dry.is_err(), "{point}: a dry run does not resume");

        remove_ticket(&plan, &machine, "2").unwrap_or_else(|err| panic!("{point}: resume: {err:?}"));
        assert!(!marker.exists(), "{point}: the marker is cleared");
        let text = fs::read_to_string(&plan).unwrap();
        assert!(!text.contains("Mistake") && text.contains("### Task 1: Keep"), "{point}:\n{text}");
        assert!(text.contains("retiredTickets") && text.contains("plan.2"), "{point}:\n{text}");
        assert!(!dir.path().join("runtime/exports/plan.2").exists(), "{point}: residue cleaned");
        load_plan(&plan).unwrap_or_else(|err| panic!("{point}: the project reads again: {err:?}"));
    }
}

/// A resumed removal accepts only the recorded before or after image; any other
/// change stops it, naming the file, and leaves the marker. §FS-rhei-remove.6.2
#[test]
fn a_resumed_removal_refuses_a_file_changed_by_something_else() {
    let (dir, plan, machine) = remove_fixture();
    let marker = dir.path().join(rhei_core::root_access::REMOVAL_MARKER);
    interrupt_removal_at("removal-validate");
    remove_ticket(&plan, &machine, "2").unwrap_err();
    clear_removal_interrupt();
    let mut text = fs::read_to_string(&plan).unwrap();
    text.push_str("\n<!-- edited by hand -->\n");
    fs::write(&plan, &text).unwrap();
    let error = remove_ticket(&plan, &machine, "2").unwrap_err().to_string();
    assert!(error.contains("neither its recorded before nor after image"), "{error}");
    assert!(marker.is_file(), "the marker stays for a person to decide");
    assert_eq!(fs::read_to_string(&plan).unwrap(), text, "nothing was overwritten");
}

/// A removal that would leave the project invalid puts every byte back and
/// clears its marker. §FS-rhei-remove.4.1
#[test]
fn a_removal_that_would_not_validate_is_rolled_back() {
    let (dir, plan, machine) = remove_fixture();
    // An after-image whose state the machine does not declare: an error the
    // baseline does not have.
    let mut images = RemovalImages::default();
    let before = fs::read_to_string(&plan).unwrap();
    images.0.push(RemovalImage {
        path: plan.clone(),
        before: Some(before.clone()),
        after: before.replace("### Task 1: Keep\n**State:** pending\n", "### Task 1: Keep\n**State:** nowhere\n"),
    });
    let marker = dir.path().join(rhei_core::root_access::REMOVAL_MARKER);
    let _owned = rhei_core::root_access::own_pending_removal(dir.path());
    forced_replace(&marker, b"{}").unwrap();
    let baseline = removal_validation_errors(&plan, Some(&machine));
    let error = commit_removal(&plan, Some(&machine), &marker, "plan.2", &images, &[], &baseline)
        .unwrap_err()
        .to_string();
    assert!(error.contains("would not validate"), "{error}");
    assert_eq!(fs::read_to_string(&plan).unwrap(), before, "every byte is put back");
    assert!(!marker.exists(), "the marker is cleared with the rollback");
}

/// The marker records every image and residue path, and reads back exactly —
/// including a file that did not exist before. §FS-rhei-remove.6.2
#[test]
fn the_pending_removal_marker_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let images = RemovalImages(vec![
        RemovalImage { path: dir.path().join("a.md"), before: Some("old\n".into()), after: "new\n".into() },
        RemovalImage { path: dir.path().join("b.md"), before: None, after: "made\n".into() },
    ]);
    let residue = vec![dir.path().join("runtime/exports/p.2")];
    let marker = dir.path().join("marker.json");
    fs::write(&marker, serde_json::to_vec(&removal_marker_json("p.2", &images, &residue)).unwrap()).unwrap();
    let (ticket, read, read_residue) = read_removal_marker(&marker).unwrap();
    assert_eq!(ticket, "p.2");
    assert_eq!(read.0, images.0);
    assert_eq!(read_residue, residue);

    fs::write(&marker, b"{not json").unwrap();
    let error = read_removal_marker(&marker).unwrap_err().to_string();
    assert!(error.contains("cannot read pending removal"), "{error}");
}

/// Numbering counts a retired sibling as taken, and `--id` cannot name it.
/// §FS-rhei-new.4 §FS-rhei-remove.5.2
#[test]
fn allocation_skips_and_refuses_retired_ids() {
    let siblings = vec!["1".to_string()];
    let retired = vec!["2".to_string()];
    assert_eq!(resolve_new_ticket_segment(None, &siblings, &retired, "auth").unwrap(), "3");
    assert_eq!(resolve_new_ticket_segment(None, &[], &["1".to_string()], "auth").unwrap(), "2");
    let error = resolve_new_ticket_segment(Some("2"), &siblings, &retired, "auth").unwrap_err();
    assert!(error.to_string().contains("retired"), "{error}");
    assert_eq!(resolve_new_ticket_segment(Some("fix"), &siblings, &retired, "auth").unwrap(), "fix");
}

/// Ownership is exact: `auth.1` never owns `auth.10`'s files, while a named
/// id whose prefix is another id's is ambiguous. §FS-rhei-remove.4.2
#[test]
fn ownership_never_follows_a_loose_prefix() {
    let others = vec!["auth.10".to_string(), "auth.fix-cache".to_string()];
    assert!(!prefix_is_ambiguous("task-auth.1-pending.log", "task-auth.1-", "auth.1", &others));
    assert!(prefix_is_ambiguous(
        "task-auth.fix-cache-pending.log",
        "task-auth.fix-",
        "auth.fix",
        &others
    ));
    assert!(!prefix_is_ambiguous("task-auth.fix-pending.log", "task-auth.fix-", "auth.fix", &["auth.1".into()]));
}

/// Only rhei's own pre-spawn header, for this ticket, is a log nothing ran in.
/// §FS-rhei-remove.4.2
#[test]
fn a_pre_spawn_header_is_recognized_exactly() {
    let header = "=== rhei agent log v1 ===\nstate: pending\ntask: auth.1\n===\n";
    assert!(is_pre_spawn_header(header, "auth.1"));
    assert!(is_pre_spawn_header(&header.replace("agent", "program"), "auth.1"));
    assert!(!is_pre_spawn_header(header, "auth.10"), "another ticket's header");
    assert!(!is_pre_spawn_header(&format!("{header}the agent said hello\n"), "auth.1"), "output");
    assert!(!is_pre_spawn_header("=== rhei agent log v1 ===\nstate: pending\n===\n", "auth.1"), "no task");
    assert!(!is_pre_spawn_header("hello\n", "auth.1"));
}

/// Exactly one section goes: the ones before and after, a fenced example that
/// looks like a heading, and the text between them all stay byte-for-byte.
/// §FS-rhei-remove.4.1
#[test]
fn removing_a_section_preserves_everything_else() {
    let plan = "# Rhei: P\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n\nBody A.\n\n\
        ### Task 2: B\n**State:** pending\n\n```md\n### Task 3: Not a heading\n```\n\n\
        ### Task 3: C\n**State:** pending\n\n## Notes\n\nKept.\n";
    let without_two = remove_ticket_section(plan, "2").unwrap();
    assert_eq!(
        without_two,
        "# Rhei: P\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n\nBody A.\n\n\
         ### Task 3: C\n**State:** pending\n\n## Notes\n\nKept.\n"
    );
    let without_three = remove_ticket_section(plan, "3").unwrap();
    assert!(without_three.contains("```md\n### Task 3: Not a heading\n```"), "{without_three}");
    assert!(without_three.ends_with("## Notes\n\nKept.\n"), "{without_three}");
    assert!(remove_ticket_section(plan, "9").is_none());

    let last = "# Rhei: P\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n\n### Task 2: B\n**State:** pending\n";
    assert_eq!(remove_ticket_section(last, "2").unwrap(), "# Rhei: P\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n");
}

/// Only the ticket's own metadata entry goes, and only the containers it
/// empties with it. §FS-rhei-remove.4.1
#[test]
fn removing_metadata_keeps_every_other_entry() {
    let mut metadata: Metadata = serde_yaml::from_str(
        "metadata:\n  tasks:\n    '1': {stateVisits: {a: 1}}\n    '2': {budgetTicketId: x}\n  other: kept\n",
    )
    .unwrap();
    assert!(remove_task_metadata_entry(&mut metadata, "2"));
    let rendered = serde_yaml::to_string(&metadata).unwrap();
    assert!(rendered.contains("'1'") && rendered.contains("other: kept"), "{rendered}");
    assert!(!rendered.contains("budgetTicketId"), "{rendered}");
    assert!(remove_task_metadata_entry(&mut metadata, "1"));
    let rendered = serde_yaml::to_string(&metadata).unwrap();
    assert!(!rendered.contains("tasks"), "an emptied tasks map goes: {rendered}");
    assert!(rendered.contains("other: kept"), "{rendered}");
    assert!(!remove_task_metadata_entry(&mut metadata, "1"), "nothing left to drop");

    let file = "---\nmetadata:\n  tasks:\n    '2': {budgetTicketId: x}\n---\n### Task 2: B\n**State:** pending\n";
    assert_eq!(remove_task_file_entry(file, "2").unwrap(), "### Task 2: B\n**State:** pending\n");
    let untouched = "### Task 2: B\n**State:** pending\n";
    assert_eq!(remove_task_file_entry(untouched, "2").unwrap(), untouched);
}
