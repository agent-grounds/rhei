//! `rhei new`'s guardrails: the lock that makes concurrent creates safe, the
//! rheis a create is and is not answerable for, and the checks that keep a
//! description from authoring plan structure.
// §FS-rhei-new.3.4 §FS-rhei-new.4 §FS-rhei-new.5.2

use std::fs;

use super::new_tests::{
    assert_failure, empty_project, flattened_output, new_run, project_with_rhei,
};
use super::*;

/// A project holding one working rhei and one that does not parse.
fn project_with_a_broken_sibling(prefix: &str) -> TestDir {
    let dir = project_with_rhei(prefix);
    write_fixture_file(&dir, "bad.rhei.md", "# Rhei: Bad\n\n## Tasks\n\n### Nonsense heading\n");
    dir
}

// ---------------------------------------------------------------------------
// Concurrent creates — §FS-rhei-new.4
// ---------------------------------------------------------------------------

/// Every create that exits 0 is in the file afterwards.
///
/// Without the lock this is not a numbering collision — it is silent data loss:
/// a losing writer overwrites the winner's ticket, and one that rolls back
/// restores a snapshot taken before the winner's write.
// §FS-rhei-new.4
#[test]
fn concurrent_creates_all_land() {
    let dir = project_with_rhei("new-concurrent");
    let workers = 8;

    let titles: Vec<String> = (1..=workers).map(|n| format!("Ticket {n}")).collect();
    let handles: Vec<_> = titles
        .iter()
        .map(|title| {
            let dir = dir.to_path_buf();
            let title = title.clone();
            std::thread::spawn(move || new_run(&["new", &title, "--under", "auth"], &dir))
        })
        .collect();

    let results: Vec<CliRun> = handles.into_iter().map(|h| h.join().expect("worker")).collect();
    for result in &results {
        assert_success(result);
    }

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    for title in &titles {
        assert!(plan.contains(title), "'{title}' exited 0 but is not in the plan:\n{plan}");
    }
    assert_eq!(
        plan.matches("\n### Task ").count() + usize::from(plan.starts_with("### Task ")),
        workers,
        "every create must add exactly one ticket:\n{plan}"
    );
    // Ids are handed out one per create, so the last one is the count.
    assert!(plan.contains(&format!("### Task {workers}: ")), "got:\n{plan}");
    assert_success(&new_run(&["validate"], &dir));
}

// ---------------------------------------------------------------------------
// A rhei that will not parse — §FS-rhei-new.5.2
// ---------------------------------------------------------------------------

/// §FS-rhei-new.5.2: an unreadable rhei blocks creates into it, and nothing
/// else. Basin capture is the case that proves it.
#[test]
fn an_unparseable_rhei_blocks_only_creates_into_itself() {
    let dir = project_with_a_broken_sibling("new-broken-sibling");

    let into_good = new_run(&["new", "Keep me", "--under", "auth"], &dir);
    assert_success(&into_good);
    assert!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file").contains("Keep me"));

    let captured = new_run(&["new", "Quick thought", "--under", "basin"], &dir);
    assert_success(&captured);
    assert!(dir.join("basin/001-quick-thought.md").is_file(), "basin capture must still work");

    assert_success(&new_run(&["new", "Fresh"], &dir));

    // The write is kept, and the inherited failure is named as inherited.
    assert!(into_good.stderr.contains("already failing validation"), "got: {}", into_good.stderr);
}

/// §FS-rhei-new.5.2: the rhei being written to is this create's business, so
/// its parse error is reported and nothing is written.
#[test]
fn a_create_into_an_unparseable_rhei_is_refused() {
    let dir = project_with_a_broken_sibling("new-broken-target");
    let before = fs::read_to_string(dir.join("bad.rhei.md")).expect("rhei file");

    let result = new_run(&["new", "Nope", "--under", "bad"], &dir);
    let said = flattened_output(&result);
    assert!(!result.status.success(), "got:\n{said}");
    assert!(said.contains("rhei 'bad' could not be loaded"), "got:\n{said}");
    assert!(said.contains("cannot place a ticket in a rhei it cannot read"), "got:\n{said}");
    assert_eq!(fs::read_to_string(dir.join("bad.rhei.md")).expect("rhei file"), before);
}

// ---------------------------------------------------------------------------
// What a description may contain — §FS-rhei-new.3.4
// ---------------------------------------------------------------------------

/// §FS-rhei-new.3.4: a heading in a description would author a second ticket.
/// Refused against the flag, before anything is written.
#[test]
fn a_heading_in_a_description_is_refused_as_an_argument() {
    let dir = project_with_rhei("new-desc-heading");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let result = new_run(
        &[
            "new",
            "Legit ticket",
            "--under",
            "auth",
            "--description",
            "Some real description.\n\n### Task 9: Injected ticket\n**State:** completed\n",
        ],
        &dir,
    );
    let said = flattened_output(&result);
    assert!(!result.status.success(), "an injected ticket must be refused:\n{said}");
    assert!(said.contains("line 3 of --description"), "the line is named:\n{said}");
    assert!(said.contains("### Task 9: Injected ticket"), "the line is shown:\n{said}");
    // No file, no line number, no code frame: the offending text is an argument.
    assert!(!said.contains("PARSE ERROR"), "got:\n{said}");
    assert!(!said.contains("auth.rhei.md"), "got:\n{said}");
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);
}

/// §FS-rhei-new.3.4: the same check guards `--description-file`, whose whole
/// purpose is piping in text that carries `### ` headings.
#[test]
fn a_heading_from_a_description_file_is_refused_too() {
    let dir = project_with_rhei("new-desc-file");
    write_fixture_file(&dir, "issue.md", "Body.\n\n## Steps to reproduce\n\n1. Do the thing\n");

    let result = new_run(
        &["new", "From an issue", "--under", "auth", "--description-file", "issue.md"],
        &dir,
    );
    assert_failure(&result, "would be read as plan structure");
    assert!(
        !dir.join("auth.rhei.md").exists() || {
            !fs::read_to_string(dir.join("auth.rhei.md"))
                .expect("rhei file")
                .contains("From an issue")
        }
    );
}

/// §FS-rhei-new.3.4: a `**Field:**` line would become metadata of the node it
/// lands in.
#[test]
fn a_metadata_marker_in_a_description_is_refused() {
    let dir = project_with_rhei("new-desc-metadata");
    assert_failure(
        &new_run(&["new", "X", "--under", "auth", "--description", "**State:** completed"], &dir),
        "would be read as plan structure",
    );
}

/// §FS-rhei-new.3.4: fenced lines are content, and stay accepted exactly as
/// written — the description is never rewritten to make it fit.
#[test]
fn a_fenced_heading_in_a_description_is_kept_verbatim() {
    let dir = project_with_rhei("new-desc-fenced");
    let description = "The shape is:\n\n```\n### Task 9: Example\n```\n\nUse it as a model.";
    assert_success(&new_run(
        &["new", "Fenced", "--under", "auth", "--description", description],
        &dir,
    ));

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert!(plan.contains("### Task 9: Example"), "the fenced line is written as given:\n{plan}");
    assert_success(&new_run(&["validate"], &dir));

    // One ticket, not two: the fenced block is prose to the parser as well.
    let listed = new_run(&["list"], &dir);
    assert_success(&listed);
    assert!(!listed.stdout.contains("auth.9"), "got: {}", listed.stdout);
}

// ---------------------------------------------------------------------------
// Argument shapes and notes — §FS-rhei-new.3.3 §FS-rhei-new.5.4
// ---------------------------------------------------------------------------

/// §FS-rhei-new.3.3: the reference flags are checked before the write, so the
/// message is about the flag rather than about a line the write produced.
#[test]
fn malformed_reference_flags_are_refused_up_front() {
    let dir = project_with_rhei("new-refs");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let consumes = new_run(&["new", "X", "--under", "auth", "--consumes", "auth.1"], &dir);
    let said = flattened_output(&consumes);
    assert!(!consumes.status.success(), "got:\n{said}");
    assert!(said.contains("--consumes 'auth.1' is not a valid reference"), "got:\n{said}");
    assert!(!said.contains("Malformed **Consumes:**"), "not a parse error:\n{said}");

    assert_failure(
        &new_run(&["new", "X", "--under", "auth", "--provides", "not a name"], &dir),
        "is not a valid export name",
    );
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);
}

/// §FS-rhei-new.5.4: an assignee is a claim, and the create says so — except
/// under `--json`, where stdout carries one object.
#[test]
fn an_assignee_is_reported_as_a_claim() {
    let dir = project_with_rhei("new-assignee-note");
    let result = new_run(&["new", "Claimed", "--under", "auth", "--assignee", "alice"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("marks the ticket claimed"), "got: {}", result.stdout);
    assert!(result.stdout.contains("rhei release"), "got: {}", result.stdout);

    let json =
        new_run(&["new", "Claimed2", "--under", "auth", "--assignee", "bob", "--json"], &dir);
    assert_success(&json);
    serde_json::from_str::<serde_json::Value>(json.stdout.trim())
        .expect("--json must emit one object and nothing else");
}

/// §FS-rhei-new.5.4: a ticket create points at the commands that read what is
/// now there, the way a rhei create points at its first ticket.
#[test]
fn a_ticket_create_names_the_next_command() {
    let dir = project_with_rhei("new-ticket-next");
    let result = new_run(&["new", "First", "--under", "auth"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Next: "), "got: {}", result.stdout);
    assert!(result.stdout.contains("rhei next"), "got: {}", result.stdout);
}

/// §FS-rhei-new.4: `index` would write the filename that marks a Directory
/// Workspace, so it is refused the way `basin` is.
#[test]
fn refuses_the_reserved_index_id() {
    let dir = empty_project("new-rhei-index");
    assert_failure(&new_run(&["new", "Index", "--id", "index"], &dir), "reserved rhei id");
    assert!(!dir.join("index.rhei.md").exists());
}

/// §FS-rhei-new.3.3: a rhei that does not declare `task` makes `--kind`
/// required, and the refusal says that rather than blaming a word the user
/// never typed.
#[test]
fn a_rhei_without_task_asks_for_kind_rather_than_blaming_it() {
    let dir = empty_project("new-kind-required");
    assert_success(&new_run(
        &["new", "Product", "--id", "prod", "--node-kinds", "epic,story"],
        &dir,
    ));

    let result = new_run(&["new", "X", "--under", "prod"], &dir);
    let said = flattened_output(&result);
    assert!(!result.status.success(), "got:\n{said}");
    assert!(said.contains("requires --kind"), "got:\n{said}");
    assert!(said.contains("epic, story"), "got:\n{said}");
    assert!(!said.contains("node kind 'task'"), "the user never typed 'task':\n{said}");
}

/// §FS-rhei-new.3.3: a rhei created without `--max-levels` has no frontmatter,
/// so the refusal spells out the block to add.
#[test]
fn the_depth_refusal_spells_out_the_missing_frontmatter() {
    let dir = project_with_rhei("new-depth-help");
    assert_success(&new_run(&["new", "One", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Two", "--under", "auth.1"], &dir));

    let result = new_run(&["new", "Three", "--under", "auth.1.1"], &dir);
    let said = flattened_output(&result);
    assert!(!result.status.success(), "got:\n{said}");
    assert!(said.contains("declares no frontmatter"), "got:\n{said}");
    assert!(said.contains("maxLevels:"), "got:\n{said}");
}

/// §FS-rhei-new.1.1: `--description-file` is being *read*, so a missing path is
/// not an invitation to create its directory.
#[test]
fn a_missing_description_file_is_reported_as_a_read() {
    let dir = project_with_rhei("new-desc-missing");
    let result =
        new_run(&["new", "X", "--under", "auth", "--description-file", "nope/nope.md"], &dir);
    let said = flattened_output(&result);
    assert!(!result.status.success(), "got:\n{said}");
    assert!(said.contains("no file there to read"), "got:\n{said}");
    assert!(!said.contains("mkdir -p"), "reading a file is not creating one:\n{said}");
}

// ---------------------------------------------------------------------------
// `rhei list` names an empty rhei — §FS-rhei-list.4.1
// ---------------------------------------------------------------------------

/// §FS-rhei-list.4.1: `rhei init` points at `rhei new`, which makes the next
/// `rhei list` the moment a project holds one rhei and no tickets.
#[test]
fn list_names_a_rhei_that_holds_no_tickets() {
    let dir = project_with_rhei("new-list-empty");
    assert_success(&new_run(&["new", "Beta"], &dir));

    let empty = new_run(&["list"], &dir);
    assert_success(&empty);
    // The heading is the progress report's: the title, carrying the id when
    // the two diverge. §FS-rhei-render.3.4
    assert!(
        empty.stdout.contains("Authentication (auth): (no tickets yet)"),
        "got: {}",
        empty.stdout
    );
    assert!(empty.stdout.contains("Beta: (no tickets yet)"), "got: {}", empty.stdout);

    assert_success(&new_run(&["new", "Rotate keys", "--under", "auth"], &dir));
    let mixed = new_run(&["list"], &dir);
    assert_success(&mixed);
    assert!(mixed.stdout.contains("Task auth.1"), "got: {}", mixed.stdout);
    assert!(mixed.stdout.contains("Beta: (no tickets yet)"), "got: {}", mixed.stdout);
    assert!(!mixed.stdout.contains("(auth): (no tickets yet)"), "got: {}", mixed.stdout);

    // `--rhei` is a filter, but an empty rhei has no answer for any filter:
    // "no tasks match the given filters" reads as "your filters are wrong"
    // about a rhei that simply holds nothing. §FS-rhei-list.4.1
    let narrowed = new_run(&["list", "--rhei", "beta"], &dir);
    assert_success(&narrowed);
    assert!(
        narrowed.stdout.contains("(rhei 'beta' holds no tickets yet)"),
        "got: {}",
        narrowed.stdout
    );
    let matching = new_run(&["list", "--rhei", "auth", "--state", "nonesuch-state"], &dir);
    assert!(
        !matching.status.success() || !matching.stdout.contains("holds no tickets"),
        "a rhei that does hold tickets keeps the filter wording: {}",
        matching.stdout
    );

    // A filter asks a question about tickets, and JSON's shape is a contract.
    let filtered = new_run(&["list", "--state", "pending"], &dir);
    assert_success(&filtered);
    assert!(!filtered.stdout.contains("no tickets yet"), "got: {}", filtered.stdout);

    let json = new_run(&["list", "--json"], &dir);
    assert_success(&json);
    let value: serde_json::Value =
        serde_json::from_str(json.stdout.trim()).expect("--json must emit an array");
    assert_eq!(value.as_array().expect("array").len(), 1, "got: {value}");
}

/// An unbalanced ``` fence is the shape a pasted issue body takes when it was
/// cut short, and it is the most destructive thing a description can carry —
/// every ticket after the insertion point stops being a plan node. Refused
/// against the flag, naming the line the fence opened on.
// §FS-rhei-new.3.4
#[test]
fn an_unclosed_fence_in_a_description_is_refused_and_loses_no_ticket() {
    let dir = project_with_rhei("new-desc-fence");
    for title in ["Alpha", "Beta", "Gamma"] {
        assert_success(&new_run(&["new", title, "--under", "auth"], &dir));
    }
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let issue = dir.join("issue.md");
    fs::write(&issue, "Repro steps:\n\n```sh\nrhei run\n").expect("issue body");
    let result = new_run(
        &[
            "new",
            "From issue",
            "--under",
            "auth.1",
            "--description-file",
            issue.to_str().expect("path"),
        ],
        &dir,
    );

    let said = flattened_output(&result);
    assert!(!result.status.success(), "got:\n{said}");
    assert!(said.contains("code fence opened on line 3"), "got:\n{said}");
    assert!(said.contains("never closed"), "got:\n{said}");
    assert_eq!(
        fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"),
        before,
        "nothing may be written, and no sibling may be swallowed"
    );
}

/// §FS-rhei-new.3.4: a rhei's description is written directly under
/// `# Rhei: <title>`, which is where the parser looks for frontmatter — so a
/// `---` line there authors `structure:` rather than describing the rhei.
#[test]
fn a_frontmatter_opener_in_a_description_is_refused() {
    let dir = empty_project("new-desc-frontmatter");
    let body = dir.join("body.md");
    fs::write(&body, "---\nstructure:\n  nodeKinds: [epic]\n---\n\nReal prose.\n")
        .expect("description body");

    let result =
        new_run(&["new", "Billing", "--description-file", body.to_str().expect("path")], &dir);
    let said = flattened_output(&result);
    assert!(!result.status.success(), "got:\n{said}");
    assert!(said.contains("frontmatter block"), "got:\n{said}");
    assert!(!dir.join("billing.rhei.md").exists(), "nothing may be written");
}

/// §FS-rhei-new.3.4: `TITLE` becomes the rest of a heading line, so an empty or
/// multi-line one is an argument error rather than a code frame in a file the
/// rollback has already removed.
#[test]
fn an_unusable_title_is_refused_as_an_argument() {
    let dir = project_with_rhei("new-title-guard");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let empty = new_run(&["new", "   ", "--under", "auth"], &dir);
    assert_failure(&empty, "TITLE is empty");

    let multi = new_run(&["new", "First line\nsecond line", "--under", "auth"], &dir);
    assert_failure(&multi, "more than one line");

    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);
}

/// §FS-rhei-new.3.4.1: a `--description` value beginning with `-` is refused by
/// argument parsing, and the refusal names the spellings that keep it instead of
/// the `--` advice, which cannot attach a value to an option and fails again
/// when it is followed.
#[test]
fn a_hyphen_leading_description_value_names_the_spellings_that_work() {
    let dir = project_with_rhei("new-desc-hyphen");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let refused =
        new_run(&["new", "t", "--under", "auth", "--description", "- Context: x.\n- y."], &dir);
    let said = flattened_output(&refused);
    // The value stays refused: accepting it would take a following flag for the
    // body too, so `--description --dry-run` would write `--dry-run` and exit 0.
    assert_eq!(refused.status.code(), Some(2), "got:\n{said}");
    assert_eq!(
        fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"),
        before,
        "a refused argument writes nothing"
    );
    assert!(said.contains("--description="), "must name the attached form, got:\n{said}");
    assert!(said.contains("--description-file -"), "must name standard input, got:\n{said}");
    // Replaced in its own slot, not accompanied: two tips of which the first is
    // wrong leave the caller choosing between them.
    assert!(!said.contains("-- - "), "the `--` advice must be gone, got:\n{said}");

    // A path is the same refusal in its own words: a bare `-` there already
    // means standard input, so piping is no answer to it.
    let path = new_run(&["new", "t", "--under", "auth", "--description-file", "-x/body.md"], &dir);
    let path_said = flattened_output(&path);
    assert_eq!(path.status.code(), Some(2), "got:\n{path_said}");
    assert!(path_said.contains("--description-file="), "got:\n{path_said}");
    assert!(!path_said.contains("-- -x"), "the `--` advice must be gone, got:\n{path_said}");
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);
}

/// §FS-rhei-new.3.4.1: a hyphen-leading `TITLE` renders byte-identically to a
/// refused `--description` value, so only the command line tells them apart — and
/// it is a different mistake with a different answer. A regression guard: this
/// holds before the change and must still hold after it.
#[test]
fn a_hyphen_leading_title_keeps_the_refusal_it_prints_today() {
    let dir = project_with_rhei("new-title-hyphen");
    let refused = new_run(&["new", "- leading", "--under", "auth"], &dir);
    let said = flattened_output(&refused);
    assert_eq!(refused.status.code(), Some(2), "got:\n{said}");
    assert!(said.contains("unexpected argument '- ' found"), "got:\n{said}");
    assert!(said.contains("use '-- - '"), "the title keeps its own advice, got:\n{said}");
    assert!(!said.contains("--description"), "a title is not an option value, got:\n{said}");
}

/// §FS-rhei-new.5.2: a `**Prior:**` into a rhei the lenient load skipped can be
/// checked by nothing, so it is refused rather than written and then blamed on
/// whoever repairs the sibling.
#[test]
fn a_prior_into_an_unreadable_rhei_is_refused() {
    let dir = project_with_a_broken_sibling("new-prior-broken");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let result = new_run(&["new", "Needs bad", "--under", "auth", "--prior", "bad.99"], &dir);
    let said = flattened_output(&result);
    assert!(!result.status.success(), "got:\n{said}");
    assert!(said.contains("rhei 'bad' could not be loaded"), "got:\n{said}");
    assert!(said.contains("--prior 'bad.99' points into rhei 'bad'"), "got:\n{said}");
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);

    // The same for the other reference flag.
    let consumes =
        new_run(&["new", "Reads bad", "--under", "auth", "--consumes", "bad.1:api"], &dir);
    assert_failure(&consumes, "--consumes 'bad.1' points into rhei 'bad'");
}

/// §FS-rhei-new.5.2: one rhei declaring a machine no `states.yaml` provides is
/// somebody else's half-finished edit. It must not stop creates elsewhere —
/// basin capture least of all — and it still must stop a create into itself.
#[test]
fn an_unresolvable_machine_elsewhere_does_not_stop_a_create() {
    let dir = project_with_rhei("new-machine-elsewhere");
    write_fixture_file(
        &dir,
        "billing.rhei.md",
        "# Rhei: Billing\n**States:** billing-review\n\n## Tasks\n",
    );

    assert_success(&new_run(&["new", "Keep going", "--under", "auth"], &dir));
    let captured = new_run(&["new", "Quick thought", "--under", "basin"], &dir);
    assert_success(&captured);
    assert!(dir.join("basin/001-quick-thought.md").is_file(), "basin capture must still work");
    assert_success(&new_run(&["new", "Another rhei"], &dir));

    // Into the rhei whose machine cannot be resolved, the failure is this
    // create's: the ticket's starting state comes out of that machine.
    let into_it = new_run(&["new", "Nope", "--under", "billing"], &dir);
    assert_failure(&into_it, "declares state machine 'billing-review'");
}

/// A create must not swallow a concurrent `rhei complete`.
///
/// The destination lock closes the window where a create rewrites a whole plan
/// while a completion rewrites one `**State:**` line in it — but the create
/// *reads* that file while deciding, before its path is known and so before the
/// lock exists. Anything landing in that window has to be noticed and the write
/// decided again, or the completion is read as absent and written over with
/// both commands exiting 0.
// §FS-rhei-new.4
#[test]
fn a_create_does_not_swallow_a_concurrent_completion() {
    let dir = project_with_rhei("new-race-complete");
    // Enough tickets that deciding the write takes long enough to overlap.
    for n in 1..=20 {
        assert_success(&new_run(&["new", &format!("Anchor {n}"), "--under", "auth"], &dir));
    }

    for round in 1..=6 {
        let creating = {
            let dir = dir.to_path_buf();
            let title = format!("Racer {round}");
            std::thread::spawn(move || new_run(&["new", &title, "--under", "auth"], &dir))
        };
        std::thread::sleep(std::time::Duration::from_millis(40));
        let completed = new_run(&["complete", &format!("auth.{round}"), "--result", "done"], &dir);
        assert_success(&creating.join().expect("create"));

        // A completion that reported success must be in the plan: the result
        // link is what `rhei complete` writes and a lost update erases.
        if completed.status.success() {
            let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
            assert!(
                plan.contains(&format!("runtime/results/auth.{round}.md")),
                "completion of auth.{round} exited 0 but the create overwrote it:\n{plan}"
            );
        }
    }

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert_eq!(
        plan.matches("\n### Task ").count(),
        26,
        "every create and completion must survive:\n{plan}"
    );
    assert_success(&new_run(&["validate"], &dir));
}
