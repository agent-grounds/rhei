/// `since` is the journal's time of the move into the gate, and only when the
/// ledger agrees that move is the ticket's latest; anything else is unknown
/// rather than a guess. §FS-rhei-gate-brief.4.4
#[test]
fn gate_since_reads_the_journalled_entry_the_ledger_confirms() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).expect("runtime");
    let write = |ledger: &str, journal: &str| {
        std::fs::write(runtime.join("state-transitions.log"), ledger).expect("ledger");
        std::fs::write(runtime.join("transitions.log"), journal).expect("journal");
    };
    let start = "2026-11-22T10:00:00Z  plan.1  start@drafting  runtime/logs/a.log\n";
    let moved = "2026-11-22T10:14:03Z  plan.1  drafting\u{2192}review  runtime/logs/a.log  exit=0\n";
    let other = "2026-11-22T11:00:00Z  plan.2  start@drafting  runtime/logs/b.log\n";

    write("plan.1 todo@drafting\nplan.1 drafting@review\n", &format!("{start}{moved}{other}"));
    assert_eq!(gate_since(root, &runtime, "plan.1", "review").as_deref(), Some("2026-11-22T10:14:03Z"));

    // A release that ended where it started, after the ticket moved itself.
    let ended = "2026-11-22T10:20:00Z  plan.1  end@drafting  runtime/logs/a.log  outcome=completed\n";
    write("plan.1 drafting@review\n", &format!("{start}{ended}"));
    assert_eq!(gate_since(root, &runtime, "plan.1", "review").as_deref(), Some("2026-11-22T10:20:00Z"));

    // The ledger's latest move is elsewhere: the ticket is not where the journal says.
    write("plan.1 drafting@review\nplan.1 review@drafting\n", &format!("{start}{moved}"));
    assert_eq!(gate_since(root, &runtime, "plan.1", "review"), None);

    // Moved by hand after the run: the journal's latest line is not this entry.
    write("plan.1 drafting@parked\nplan.1 parked@review\n", &format!("{start}{moved}"));
    assert_eq!(gate_since(root, &runtime, "plan.1", "review"), None);

    // No journal at all.
    std::fs::remove_file(runtime.join("transitions.log")).expect("remove journal");
    write("plan.1 drafting@review\n", "");
    assert_eq!(gate_since(root, &runtime, "plan.1", "review"), None);
}

/// The answer path is per ticket and per visit, under the execution root's
/// runtime. §FS-rhei-gate-brief.7.1
#[test]
fn gate_answer_path_names_the_ticket_and_the_visit() {
    let view = GateView {
        ticket: "agora.1".into(),
        ticket_title: String::new(),
        state: "human-judgment".into(),
        visit: 3,
        since: None,
        plan: PathBuf::new(),
        root: PathBuf::new(),
        brief: rhei_core::gate::Brief::build(rhei_core::gate::BriefParts {
            ticket: "agora.1",
            ticket_title: "Discuss",
            state: "human-judgment",
            visit: 3,
            block: None,
            block_title: None,
            block_summary: None,
            block_display: None,
            request: None,
            destinations: &[],
        }),
        display: None,
        attachments: Vec::new(),
        destinations: Vec::new(),
    };
    assert_eq!(view.answer_path(), "runtime/gates/agora.1/3.answer.json");
    let text = render_gate_brief(&view);
    assert!(text.ends_with("rhei gate answer agora.1 --from human-judgment --answers <file|->\n"), "{text}");
}

/// A file a brief shows is reported, never refused: missing, a directory, or a
/// path that leaves the root each say why. §FS-rhei-gate-cmd.2.1
#[test]
fn gate_file_view_says_why_a_file_cannot_be_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("here.md"), "x").expect("file");
    std::fs::create_dir_all(dir.path().join("folder")).expect("folder");
    let here = gate_file_view(dir.path(), "here.md");
    assert!(here.readable && here.problem.is_none());
    assert_eq!(gate_file_view(dir.path(), "gone.md").problem.as_deref(), Some("does not exist"));
    assert_eq!(gate_file_view(dir.path(), "folder").problem.as_deref(), Some("is not a file"));
    let escaping = gate_file_view(dir.path(), "../outside.md");
    assert!(!escaping.readable);
    assert!(escaping.problem.as_deref().is_some_and(|problem| problem.contains("..")), "{escaping:?}");
}
