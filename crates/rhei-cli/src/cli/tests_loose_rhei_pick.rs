/// Which tickets may name a loose rhei: a dotted id led by a name, and
/// `--under`'s single rhei id; a rhei-local id or a lone name given as a
/// ticket names none. §FS-rhei-panta.6.7
#[test]
fn loose_pick_eligibility_follows_the_prior_qualification_shape() {
    let prefix = |by, ticket: &str| TicketPick::of(by, ticket).rhei_prefix().map(str::to_string);
    let ticket = TicketPickBy::Ticket("show");
    for local in ["1", "1.2", "review", "1.review", "1..2"] {
        assert_eq!(prefix(ticket, local), None, "{local} names no rhei");
    }
    assert_eq!(prefix(ticket, "italy.1").as_deref(), Some("italy"));
    assert_eq!(prefix(ticket, "italy.review").as_deref(), Some("italy"));
    assert_eq!(prefix(TicketPickBy::NextTask, "italy.1.2").as_deref(), Some("italy"));
    assert_eq!(prefix(TicketPickBy::Under, "italy").as_deref(), Some("italy"));
    assert_eq!(prefix(TicketPickBy::Under, "italy.1").as_deref(), Some("italy"));
    assert_eq!(prefix(TicketPickBy::Under, "1"), None, "a number is no rhei id");
}

/// The forms a refusal offers keep the ticket's local part and swap the rhei.
/// §FS-rhei-panta.6.7
#[test]
fn loose_pick_offers_the_qualified_and_the_plan_form() {
    let release = TicketPick::ticket("release", "venice.1");
    assert_eq!(release.qualified_form("italy"), "rhei release italy.1");
    assert_eq!(
        release.plan_form("rome.rhei.md", "rome").as_deref(),
        Some("rhei release rome.rhei.md --task 1")
    );
    let bare = TicketPick::ticket("release", "1");
    assert_eq!(bare.qualified_form("italy"), "rhei release italy.1");
    let under = TicketPick::of(TicketPickBy::Under, "venice");
    assert_eq!(under.qualified_form("italy"), "rhei new <title> --under italy");
    assert_eq!(
        under.plan_form("italy.rhei.md", "italy").as_deref(),
        Some("rhei new <title> --project italy.rhei.md --under italy")
    );
    assert_eq!(TicketPick::of(TicketPickBy::NoteTask, "venice.1").plan_form("a", "a"), None);
}

/// Zero, one and two entries with the prefix as their id; an entry whose id
/// cannot be derived matches nothing and aborts nothing. §FS-rhei-panta.6.7
#[test]
fn loose_pick_counts_the_entries_with_the_prefix_as_their_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = |name: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, "# Rhei: X\n\n## Tasks\n").expect("write");
        path
    };
    let italy = file("italy.rhei.md");
    let rome = file("rome.rhei.md");
    let basin = file("basin.rhei.md");
    let invalid = file("not.valid.rhei.md");
    let italy_dir = dir.path().join("italy");
    std::fs::create_dir(&italy_dir).expect("mkdir");
    std::fs::write(italy_dir.join("index.rhei.md"), "# Rhei: Italy\n").expect("write");

    let loose = vec![italy.clone(), rome.clone(), basin.clone(), invalid.clone()];
    assert_eq!(loose_rheis_with_id(&loose, "venice"), Vec::<&PathBuf>::new());
    assert_eq!(loose_rheis_with_id(&loose, "italy"), vec![&italy]);
    assert_eq!(loose_rheis_with_id(&loose, "basin"), Vec::<&PathBuf>::new());
    let shared = vec![italy.clone(), italy_dir.clone(), rome];
    assert_eq!(loose_rheis_with_id(&shared, "italy"), vec![&italy, &italy_dir]);

    let pick = TicketPick::ticket("show", "italy.1");
    let picked = pick_loose_rhei(dir.path(), loose.clone(), Some(&pick)).expect("one match");
    assert_eq!(picked, italy);
    let shared_err = pick_loose_rhei(dir.path(), shared, Some(&pick)).expect_err("two");
    assert!(shared_err.to_string().contains("two rheis"), "{shared_err}");
    let none = TicketPick::ticket("show", "venice.1");
    let none_err = pick_loose_rhei(dir.path(), loose.clone(), Some(&none)).expect_err("none");
    assert!(none_err.to_string().contains("no rhei here has the id `venice`"), "{none_err}");
    assert!(none_err.to_string().contains("not.valid.rhei.md"), "{none_err}");
    let ticketless = pick_loose_rhei(dir.path(), loose, None).expect_err("no ticket");
    assert!(ticketless.to_string().contains("no single plan to pick"), "{ticketless}");
}
