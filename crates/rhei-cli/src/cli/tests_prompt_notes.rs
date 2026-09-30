    // The fold and the block, read straight off the store's bytes: what a
    // second record does to the first, what a strike and a restatement do to
    // someone else's, and what the cap leaves behind.

    // §FS-rhei-memory.4.2 §FS-rhei-memory.4.5 §FS-rhei-note.3.2

    /// The entries the fold left, as `task|text`, oldest first.
    fn folded(contents: &str) -> Vec<String> {
        fold_note_store(contents)
            .into_iter()
            .map(|entry| format!("{}|{}", entry.task, entry.text))
            .collect()
    }

    /// §FS-rhei-memory.4.2: only a task's last record counts, and it is the
    /// fold that says so — the file keeps both.
    #[test]
    fn a_second_record_by_one_task_supersedes_the_first() {
        let store = "- [auth.2] the first thing\n- [auth.1] a fact of its own\n\
                     - [auth.2] what it actually was\n";
        assert_eq!(folded(store), vec!["auth.1|a fact of its own", "auth.2|what it actually was"]);
    }

    /// §FS-rhei-memory.4.2: a `restates` moves the entry to the restating
    /// record's own position, which is what makes it the newest.
    #[test]
    fn a_restatement_moves_an_entry_to_the_restating_records_position() {
        let store = "- [auth.1] the older fact\n- [auth.2] the newer fact\n\
                     - [billing.1 restates auth.1]\n";
        assert_eq!(
            folded(store),
            vec!["auth.2|the newer fact", "auth.1|the older fact"],
            "the restated entry is last in file order, so it renders first"
        );
    }

    /// §FS-rhei-memory.4.2: a `strikes` takes the entry out of composition —
    /// and §FS-rhei-note.3.4 leaves its bytes exactly where they were.
    #[test]
    fn a_strike_removes_an_entry_from_composition_only() {
        let store = "- [auth.1] a fact worth keeping\n- [auth.2] a fact worth dropping\n\
                     - [reporting.1 strikes auth.2]\n";
        assert_eq!(folded(store), vec!["auth.1|a fact worth keeping"]);
        assert!(store.contains("a fact worth dropping"), "the file is never rewritten");
    }

    /// §FS-rhei-memory.4.2: a record naming an entry that is not live at that
    /// point is inert — not an error, and not a resurrection.
    #[test]
    fn a_record_naming_no_live_entry_is_inert() {
        let store = "- [auth.1] the only fact\n- [billing.1 restates auth.9]\n\
                     - [reporting.1 strikes auth.9]\n";
        assert_eq!(folded(store), vec!["auth.1|the only fact"]);
    }

    /// §FS-rhei-memory.4.2: a task that strikes an entry and then records one
    /// of its own supersedes its own strike, and the struck entry comes back.
    /// It falls out of the one rule rather than needing a second.
    #[test]
    fn a_strike_superseded_by_its_writers_later_record_brings_the_entry_back() {
        let store = "- [auth.2] the disputed fact\n- [reporting.1 strikes auth.2]\n\
                     - [reporting.1] I was wrong about something else\n";
        assert_eq!(
            folded(store),
            vec!["auth.2|the disputed fact", "reporting.1|I was wrong about something else"]
        );
    }

    /// §FS-rhei-memory.4.2: a record composition cannot parse is skipped, not
    /// fatal. A prompt must compose.
    #[test]
    fn an_unparsable_record_is_skipped_rather_than_fatal() {
        let store = "- [auth.1] a fact\nnot a list item at all\n- no bracket here\n\
                     - [] empty id\n- [auth.2] another fact\n";
        assert_eq!(folded(store), vec!["auth.1|a fact", "auth.2|another fact"]);
    }

    /// §FS-rhei-note.3.2: the bracket is a closed grammar, so a note whose own
    /// text begins with a verb is still a note.
    #[test]
    fn an_entry_whose_text_begins_with_a_verb_is_not_a_verb() {
        let store = "- [auth.1] strikes against the build are cheap to fix\n";
        assert_eq!(folded(store), vec!["auth.1|strikes against the build are cheap to fix"]);
    }

    /// §FS-rhei-memory.4.2: rendered newest first — descending file position.
    #[test]
    fn the_block_renders_newest_first() {
        let store = "- [auth.1] the older fact\n- [auth.2] the newer fact\n";
        let block = render_project_notes(&fold_note_store(store), "runtime/notes.md");
        assert!(block.contains("### Project Notes\n"), "got:\n{block}");
        assert!(
            block.contains(
                "Facts earlier tickets left for whoever came next, newest first.\n"
            ),
            "got:\n{block}"
        );
        let newer = block.find("- [auth.2] the newer fact").expect("the newer entry");
        let older = block.find("- [auth.1] the older fact").expect("the older entry");
        assert!(newer < older, "got:\n{block}");
    }

    /// §FS-rhei-memory.4.5: 24 lines of entries, with the overflow line first
    /// and the count of what it dropped.
    #[test]
    fn the_block_is_capped_at_24_lines_with_the_overflow_line_first() {
        let mut store = String::new();
        for n in 1..=30 {
            store.push_str(&format!("- [plan.{n}] fact number {n}\n"));
        }
        let block = render_project_notes(&fold_note_store(&store), "runtime/notes.md");
        let overflow = "\u{2026} 6 earlier notes not shown \u{2014} read runtime/notes.md\n";
        assert!(block.contains(overflow), "got:\n{block}");
        let entries: Vec<&str> =
            block.lines().filter(|line| line.starts_with("- [plan.")).collect();
        assert_eq!(entries.len(), 24, "got:\n{block}");
        // Newest first, so the 24 kept are the last 24 written.
        assert!(block.contains("- [plan.30] fact number 30\n"), "got:\n{block}");
        assert!(!block.contains("- [plan.6] fact number 6\n"), "got:\n{block}");
        assert!(
            block.find(overflow) < block.find("- [plan.30]"),
            "the overflow line comes first; got:\n{block}"
        );
    }

    /// §FS-rhei-memory.3.1: nothing live is no block, not an empty one.
    #[test]
    fn an_empty_store_renders_no_block() {
        assert_eq!(render_project_notes(&fold_note_store(""), "runtime/notes.md"), "");
        let struck = "- [auth.1] the only fact\n- [auth.2 strikes auth.1]\n";
        assert_eq!(render_project_notes(&fold_note_store(struck), "runtime/notes.md"), "");
    }
