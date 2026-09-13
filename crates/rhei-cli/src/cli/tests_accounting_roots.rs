// What a reading over several accounting roots enumerates, drops, and prices.
// The end-to-end scenarios drive the whole command; these pin the rules that a
// black-box run cannot address one at a time — and the unreadable root, which
// cannot be reached portably from outside at all.

// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.5.2 §FS-rhei-cost-accounting.6.2

/// A price book both fixture roots name, at whatever rate is passed, so that
/// two roots can disagree about one `price_book_id`.
fn roots_price_book(input_total_micro: u64) -> PriceBook {
    PriceBook {
        schema: ACCOUNTING_PRICES_SCHEMA.to_string(),
        price_book_id: "roots-fixture".to_string(),
        currency: "USD".to_string(),
        entries: vec![PriceBookEntry {
            provider: "openai".to_string(),
            model: "gpt-test".to_string(),
            effective_at: "2026-01-01T00:00:00Z".to_string(),
            unit: "1m_tokens".to_string(),
            input_total_micro,
            input_cached_read_micro: input_total_micro / 10,
            input_cache_write_micro: input_total_micro * 5 / 4,
            output_total_micro: input_total_micro * 5,
            extensions: BTreeMap::new(),
        }],
        extensions: BTreeMap::new(),
    }
}

/// A measured, priced record with cache dimensions inside `input.total` and no
/// stated convention, so a reading recomputes its money against the book of the
/// root it was read from. §FS-rhei-cost-accounting.5.2
fn roots_record(invocation_id: &str, task_id: &str, started_at: &str) -> AccountingInvocationRecord {
    let mut record = AccountingInvocationRecord {
        invocation_id: invocation_id.to_string(),
        task_id: task_id.to_string(),
        started_at: started_at.to_string(),
        ended_at: started_at.to_string(),
        ..accounting_test_record()
    };
    record.tokens.total = AccountingTokenDimension::measured(1_000);
    record.tokens.input.total = AccountingTokenDimension::measured(800);
    record.tokens.input.cached_read = AccountingTokenDimension::measured(300);
    record.tokens.input.cache_write = AccountingTokenDimension::measured(100);
    record.tokens.output.total = AccountingTokenDimension::measured(200);
    record.pricing.status = "priced".to_string();
    record.pricing.amount_micro = Some(1);
    record.pricing.priced_amount_micro = Some(1);
    record.pricing.price_book_id = Some("roots-fixture".to_string());
    record
}

/// Write one record into `<root>/runtime/accounting/invocations/`.
fn write_roots_record(root: &Path, record: &AccountingInvocationRecord) {
    let dir = root.join(ACCOUNTING_DIR).join("invocations");
    fs::create_dir_all(&dir).expect("invocations directory");
    fs::write(
        dir.join(format!("{}.json", record.invocation_id)),
        serde_json::to_vec_pretty(record).expect("serialize record"),
    )
    .expect("write record");
}

/// The enumerated root for one execution root, as `accounting_roots` builds it.
fn roots_entry(root: &Path, rheis: &[&str], shared: bool) -> AccountingRoot {
    AccountingRoot {
        path: root.join(ACCOUNTING_DIR),
        rheis: rheis.iter().map(|id| (*id).to_string()).collect(),
        shared,
    }
}

/// A Panta project holding every shape the enumeration has to get right: a
/// Directory Workspace member, two single-file rheis whose execution root is
/// the project directory, a `basin`, and a member that has never been run.
fn roots_project(project: &Path) {
    fs::create_dir_all(project).expect("project directory");
    fs::write(project.join("index.panta.md"), "# Panta: Roots\n").expect("manifest");
    for id in ["ledger", "spool"] {
        fs::write(
            project.join(format!("{id}.rhei.md")),
            format!("# Rhei: {id}\n\n## Tasks\n\n### Task 1: Work\n**State:** draft\n"),
        )
        .expect("single-file rhei");
    }
    for id in ["billing", "quiet"] {
        let member = project.join(id);
        fs::create_dir_all(member.join("tasks")).expect("member workspace");
        fs::write(member.join("index.rhei.md"), format!("# Rhei: {id}\n")).expect("member index");
        fs::write(member.join("tasks/01-work.md"), "### Task 1: Work\n**State:** draft\n")
            .expect("member task");
    }
    fs::create_dir_all(project.join("basin")).expect("basin");
    fs::write(project.join("basin/01-unfiled.md"), "### Task 1: Unfiled\n**State:** draft\n")
        .expect("basin ticket");
}

#[test]
fn root_enumeration_collapses_a_shared_root_and_keeps_the_basin() {
    // §FS-rhei-panta.6.5: the run root leads, every rhei's execution root
    // follows, and the two single-file rheis share the project directory, so
    // the union is four roots rather than six.
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("project");
    roots_project(&project);
    let loaded = load_plan(&project).expect("project loads");

    let roots = accounting_roots(&loaded, &project, &None);
    let paths: Vec<PathBuf> = roots.iter().map(|root| root.path.clone()).collect();
    assert_eq!(
        paths,
        vec![
            project.join(ACCOUNTING_DIR),
            project.join("billing").join(ACCOUNTING_DIR),
            project.join("quiet").join(ACCOUNTING_DIR),
            project.join("basin").join(ACCOUNTING_DIR),
        ],
        "the run root first, then each rhei's, deduplicated by path"
    );
    assert_eq!(
        roots[0].rheis,
        vec!["ledger".to_string(), "spool".to_string()],
        "both single-file rheis root at the project directory"
    );
    assert!(roots.iter().all(|root| !root.shared), "a project-wide reading shares with nobody");

    // A member that has never been run holds no accounting directory, and that
    // is an ordinary member rather than an error. §FS-rhei-panta.6.5
    let inspection = read_cost_inspection_over(&roots, &None);
    assert!(inspection.errors.is_empty(), "{:?}", inspection.errors);
    assert!(inspection.invocations.is_empty());
    assert_eq!(inspection.roots.len(), 4, "an empty root is still a root it searched");
}

#[test]
fn narrowing_marks_a_root_shared_only_while_a_rhei_outside_the_scope_holds_it() {
    // §FS-rhei-panta.6.5: the root decides when every rhei rooted there is in
    // scope; the record's `task_id` decides only where one root is shared.
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("project");
    roots_project(&project);
    let loaded = load_plan(&project).expect("project loads");

    let one = rhei_scope_set(&["ledger".to_string()]);
    let narrowed = accounting_roots(&loaded, &project, &one);
    assert_eq!(narrowed.len(), 1, "a narrowed reading covers the named rhei's root alone");
    assert_eq!(narrowed[0].path, project.join(ACCOUNTING_DIR));
    assert!(narrowed[0].shared, "`spool` roots here too and is out of scope");

    let both = rhei_scope_set(&["ledger".to_string(), "spool".to_string()]);
    assert!(
        !accounting_roots(&loaded, &project, &both)[0].shared,
        "with every rhei rooted there in scope, the root decides on its own"
    );

    let member = rhei_scope_set(&["billing".to_string()]);
    let billing = accounting_roots(&loaded, &project, &member);
    assert_eq!(billing[0].path, project.join("billing").join(ACCOUNTING_DIR));
    assert!(!billing[0].shared, "a Directory Workspace member roots alone");
}

#[test]
fn a_shared_root_reports_only_the_records_of_the_rheis_in_scope() {
    // §FS-rhei-panta.6.5: reading the shared root and filtering its records is
    // what lets `--rhei` name a single-file rhei at all.
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("project");
    roots_project(&project);
    write_roots_record(&project, &roots_record("ledger-0", "ledger.1", "2026-09-01T10:00:00Z"));
    write_roots_record(&project, &roots_record("spool-0", "spool.1", "2026-09-01T10:01:00Z"));

    let scope = rhei_scope_set(&["ledger".to_string()]);
    let inspection =
        read_cost_inspection_over(&[roots_entry(&project, &["ledger"], true)], &scope);
    let ids: Vec<&str> =
        inspection.invocations.iter().map(|held| held.record.invocation_id.as_str()).collect();
    assert_eq!(ids, vec!["ledger-0"], "`spool`'s record shares the root and stays out of scope");
    assert_eq!(inspection.roots[0].invocation_count, 1);

    // An exclusive root is not filtered, so a record written before tickets
    // were project-qualified is still counted. §FS-rhei-panta.6.3
    let member = project.join("billing");
    write_roots_record(&member, &roots_record("legacy-0", "1", "2026-09-01T10:02:00Z"));
    let scope = rhei_scope_set(&["billing".to_string()]);
    let inspection =
        read_cost_inspection_over(&[roots_entry(&member, &["billing"], false)], &scope);
    assert_eq!(inspection.invocations.len(), 1, "an unqualified id is not dropped by the filter");
}

#[test]
fn one_invocation_id_is_counted_once_and_a_disagreeing_twin_is_reported() {
    // §FS-rhei-panta.6.5: an identical repeat is dropped silently; one that
    // differs is dropped and named, because only one of them can be right.
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    let third = dir.path().join("third");
    let record = roots_record("shared-0", "billing.1", "2026-09-01T10:00:00Z");
    write_roots_record(&first, &record);
    write_roots_record(&second, &record);
    let mut differing = record.clone();
    differing.state = "review".to_string();
    write_roots_record(&third, &differing);

    let roots = [
        roots_entry(&first, &["a"], false),
        roots_entry(&second, &["b"], false),
        roots_entry(&third, &["c"], false),
    ];
    let inspection = read_cost_inspection_over(&roots, &None);
    assert_eq!(inspection.invocations.len(), 1, "one record, counted once");
    assert_eq!(inspection.roots[0].invocation_count, 1);
    assert_eq!(inspection.roots[1].invocation_count, 0, "the identical twin is dropped silently");
    assert_eq!(inspection.roots[2].invocation_count, 0);
    assert_eq!(inspection.errors.len(), 1, "only the disagreeing twin is reported: {:?}", inspection.errors);
    let reported = &inspection.errors[0];
    assert!(reported.contains("shared-0"), "the error names the invocation; got: {reported}");
    assert!(
        reported.contains(&third.display().to_string())
            && reported.contains(&first.display().to_string()),
        "the error names both paths; got: {reported}"
    );
}

#[test]
fn each_record_is_priced_by_the_book_of_the_root_it_was_read_from() {
    // §FS-rhei-cost-accounting.5.2: a record's book is the `prices.json` beside
    // it, so two roots naming one book id at different rates is not a conflict.
    let dir = tempfile::tempdir().expect("tempdir");
    let cheap_root = dir.path().join("cheap");
    let dear_root = dir.path().join("dear");
    let cheap = roots_price_book(1_000_000);
    let dear = roots_price_book(8_000_000);
    write_roots_record(&cheap_root, &roots_record("cheap-0", "a.1", "2026-09-01T10:00:00Z"));
    write_roots_record(&dear_root, &roots_record("dear-0", "b.1", "2026-09-01T10:01:00Z"));
    write_price_book(&cheap_root.join(ACCOUNTING_DIR), &cheap).expect("cheap book");
    write_price_book(&dear_root.join(ACCOUNTING_DIR), &dear).expect("dear book");

    let roots =
        [roots_entry(&cheap_root, &["a"], false), roots_entry(&dear_root, &["b"], false)];
    let inspection = read_cost_inspection_over(&roots, &None);
    let held: Vec<ScopedRecord<'_>> = inspection.scoped().collect();
    assert_eq!(held.len(), 2);

    let expected = |book: &PriceBook, record: &AccountingInvocationRecord| {
        price_tokens(book, record.provider.as_deref(), record.model.as_deref(), &record.tokens)
            .amount_micro
    };
    let read_by_own_book = |held: ScopedRecord<'_>| {
        read_stored_record(held.record, held.books).pricing.amount_micro
    };
    assert_eq!(read_by_own_book(held[0]), expected(&cheap, held[0].record));
    assert_eq!(read_by_own_book(held[1]), expected(&dear, held[1].record));
    assert_ne!(
        read_by_own_book(held[0]),
        read_by_own_book(held[1]),
        "the two books disagree, so pricing by the wrong one would show here"
    );
}

/// A root that exists and cannot be read is the one failure mode the union
/// introduces: with a single root an unreadable directory printed
/// `(no accounting records found)` and the emptiness was the report, while over
/// several it would silently shrink a total that still claimed to be whole.
///
/// Unix only, because making a directory unreadable is a Unix operation and
/// Windows is in the continuous integration matrix.
// §FS-rhei-cost-accounting.6.2 §FS-rhei-cost-accounting.11
#[cfg(unix)]
#[test]
fn an_unreadable_root_is_named_and_stops_the_reading_claiming_complete() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let readable = dir.path().join("readable");
    let blocked = dir.path().join("blocked");
    let book = roots_price_book(1_000_000);
    write_roots_record(&readable, &roots_record("readable-0", "a.1", "2026-09-01T10:00:00Z"));
    write_price_book(&readable.join(ACCOUNTING_DIR), &book).expect("book");
    let blocked_invocations = blocked.join(ACCOUNTING_DIR).join("invocations");
    fs::create_dir_all(&blocked_invocations).expect("blocked invocations");
    fs::set_permissions(&blocked_invocations, fs::Permissions::from_mode(0o000))
        .expect("make the root unreadable");
    if fs::read_dir(&blocked_invocations).is_ok() {
        // Running with privileges that ignore the mode bits; there is nothing
        // here to observe.
        return;
    }

    let roots =
        [roots_entry(&readable, &["a"], false), roots_entry(&blocked, &["b"], false)];
    let inspection = read_cost_inspection_over(&roots, &None);
    assert!(inspection.unreadable_root, "a root that exists and will not read is a boundary");
    assert!(
        inspection.errors.iter().any(|error| error.contains(&blocked.display().to_string())),
        "the reading names the root it could not read; got: {:?}",
        inspection.errors
    );
    assert_eq!(
        inspection.summary.as_ref().map(|summary| summary.coverage),
        Some(rhei_tui::UsageCoverage::Partial),
        "the rest is read, and no aggregate over it reports `complete`"
    );

    // The same reading with every root readable is what it is being compared
    // against: without the demotion this record alone is `Complete`.
    let readable_only = read_cost_inspection_over(&roots[..1], &None);
    assert_eq!(
        readable_only.summary.as_ref().map(|summary| summary.coverage),
        Some(rhei_tui::UsageCoverage::Complete)
    );

    fs::set_permissions(&blocked_invocations, fs::Permissions::from_mode(0o755))
        .expect("restore permissions so the temporary directory can be removed");
}

#[test]
fn the_union_is_ordered_by_started_at_across_the_roots_it_came_from() {
    // §FS-rhei-summary.2.2: the entries are ordered by `started_at`. Each root
    // is read in that order, so only a union appended root by root can show the
    // grouping — here the first root enumerated holds the *later* record.
    let dir = tempfile::tempdir().expect("tempdir");
    let later = dir.path().join("later");
    let earlier = dir.path().join("earlier");
    let dear = roots_price_book(8_000_000);
    let cheap = roots_price_book(1_000_000);
    write_roots_record(&later, &roots_record("later-0", "a.1", "2026-09-01T10:05:00Z"));
    write_roots_record(&later, &roots_record("later-1", "a.2", "2026-09-01T10:06:00Z"));
    write_roots_record(&earlier, &roots_record("earlier-0", "b.1", "2026-09-01T10:00:00Z"));
    write_price_book(&later.join(ACCOUNTING_DIR), &dear).expect("dear book");
    write_price_book(&earlier.join(ACCOUNTING_DIR), &cheap).expect("cheap book");

    let roots = [roots_entry(&later, &["a"], false), roots_entry(&earlier, &["b"], false)];
    let inspection = read_cost_inspection_over(&roots, &None);
    let ids: Vec<&str> =
        inspection.invocations.iter().map(|held| held.record.invocation_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["earlier-0", "later-0", "later-1"],
        "the second root's record started first and leads the reading"
    );

    // The reordering must not move what each root contributed, nor which book
    // a record is priced by: both are carried on the record, not on its
    // position. §FS-rhei-cost-accounting.5.2 §FS-rhei-cost-accounting.8.4
    assert_eq!(inspection.roots[0].invocation_count, 2, "the counts stay with their roots");
    assert_eq!(inspection.roots[1].invocation_count, 1);
    let priced = |held: &InspectedRecord| {
        read_stored_record(&held.record, inspection.books_of(held)).pricing.amount_micro
    };
    let by_book = |book: &PriceBook, record: &AccountingInvocationRecord| {
        price_tokens(book, record.provider.as_deref(), record.model.as_deref(), &record.tokens)
            .amount_micro
    };
    assert_eq!(priced(&inspection.invocations[0]), by_book(&cheap, &inspection.invocations[0].record));
    assert_eq!(priced(&inspection.invocations[1]), by_book(&dear, &inspection.invocations[1].record));
}
