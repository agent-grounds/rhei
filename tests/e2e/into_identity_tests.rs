//! What a placement does about a ticket's **budget identity**, which is the one
//! thing `--into` could get wrong in a way nobody would notice.
//! §FS-rhei-library.5 §FS-rhei-budgets.1 §REQ-bounded-neural-work.4
//!
//! A budget identity belongs to a run, never to a template, so placement does
//! one refusal and one deletion and reads nothing of the host's. Whether a
//! placed ticket travels *fresh* is then not a question about either document:
//! the project's ledger decides it, from whether it holds a binding for the pair
//! the ticket lands on. These three cases are the ones the agora left
//! unanswered, and two of them no test covers today.

use std::fs;
use std::path::Path;

use super::into_support::*;
use super::*;

/// The journal's receipts, one JSON object per line.
/// §FS-rhei-budgets.5.2
fn receipts(project_root: &Path) -> Vec<serde_json::Value> {
    let budgets = project_root.join(".agent-grounds/rhei/budgets");
    let account = fs::read_dir(&budgets)
        .unwrap_or_else(|e| panic!("read {}: {e}", budgets.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .expect("the project should have one budget account");
    fs::read_to_string(account.join("journal.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("every journal line is JSON"))
        .collect()
}

fn kinds<'a>(receipts: &'a [serde_json::Value], kind: &str) -> Vec<&'a serde_json::Value> {
    receipts.iter().filter(|r| r["kind"] == kind).collect()
}

/// Every `budgetTicketId` a plan's frontmatter carries, keyed by ticket id.
fn identities(index: &Path) -> Vec<(String, String)> {
    let text = fs::read_to_string(index).unwrap_or_else(|e| panic!("read index: {e}"));
    let mut found = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(id) = trimmed.strip_suffix(':') {
            if !id.is_empty() && !id.contains(' ') {
                current = id.to_owned();
            }
        }
        if let Some(uuid) = trimmed.strip_prefix("budgetTicketId: ") {
            found.push((current.clone(), uuid.to_owned()));
        }
    }
    found
}

// ---------------------------------------------------------------------------
// (3) `--into` refuses an identity-bearing source
// ---------------------------------------------------------------------------

/// A template that declares a `budgetTicketId` is refused before anything is
/// written, and the host's index is byte-identical afterwards. A budget
/// identity belongs to a run and never to a template.
/// §FS-rhei-library.5
#[test]
fn into_refuses_a_template_that_declares_a_budget_identity() {
    let (dir, root) = host_workspace("identity-refuse-source");
    let template = write_review_template(&dir);
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Review {{change_ref}}\n**States:** review-loop\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - step\nmetadata:\n  tasks:\n    coordinate:\n      budgetTicketId: 11111111-2222-3333-4444-555555555555\n---\n\n## Overview\n\nA review loop.\n",
    );
    let index_before = read(&root.join("index.rhei.md"));

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);

    assert!(!result.status.success(), "an identity-bearing template must be refused");
    assert_stderr_contains(&result, "budgetTicketId");
    assert_stderr_contains(&result, "coordinate");
    assert_eq!(
        index_before,
        read(&root.join("index.rhei.md")),
        "the host's index must be byte-identical after the refusal"
    );
    assert!(!root.join("tasks/002-coordinate.md").exists(), "no ticket is written");
}

/// The refusal must scan a placed **task file's** own metadata block too, not
/// only the index's — otherwise it has a hole the moment a template carries its
/// per-task metadata where §FS-rhei-plan-language.1.4 now allows it.
/// §FS-rhei-library.5 §FS-rhei-library.4
#[test]
fn the_identity_refusal_scans_a_task_files_own_metadata_block() {
    let (dir, root) = host_workspace("identity-refuse-task-file");
    let template = write_review_template(&dir);
    write_fixture_file(
        &template,
        "tasks/001-coordinate.md",
        "---\nmetadata:\n  tasks:\n    coordinate:\n      budgetTicketId: 11111111-2222-3333-4444-555555555555\n---\n\n### Step coordinate: Coordinate review of {{change_ref}}\n**State:** review\n\nReview {{change_ref}}.\n",
    );
    let index_before = read(&root.join("index.rhei.md"));

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);

    assert!(!result.status.success(), "an identity in a task file's own block must be refused");
    assert_stderr_contains(&result, "budgetTicketId");
    assert_eq!(index_before, read(&root.join("index.rhei.md")), "the target is left alone");
}

/// The refusal fires in `--output` mode as well: a budget identity belongs to a
/// run whether the template is being placed or laid standalone.
/// §FS-rhei-library.5
#[test]
fn the_identity_refusal_fires_for_output_mode_too() {
    let dir = unique_temp_dir("identity-refuse-output");
    let template = write_review_template(&dir);
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Review {{change_ref}}\n**States:** review-loop\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - step\nmetadata:\n  tasks:\n    coordinate:\n      budgetTicketId: 11111111-2222-3333-4444-555555555555\n---\n\n## Overview\n\nA review loop.\n",
    );

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--output", "out"], &dir);

    assert!(!result.status.success(), "--output must refuse an identity-bearing template too");
    assert_stderr_contains(&result, "budgetTicketId");
    assert!(!dir.join("out").exists(), "nothing is published");
}

// ---------------------------------------------------------------------------
// (2) a placement under a free id mints
// ---------------------------------------------------------------------------

/// A placement at a free id is a genuinely new ticket identity: the ledger holds
/// no binding for the pair it lands on, so the placed ticket gets a
/// `budgetTicketId` of its own, its own `identity` receipt, and a travel bound
/// the host ticket's spending has not touched.
/// §FS-rhei-library.5 §FS-rhei-budgets.1 §REQ-bounded-neural-work.4
#[test]
fn a_placement_under_a_free_id_mints_its_own_identity_and_bound() {
    let (dir, root) = host_workspace("identity-mint");
    write_review_template(&dir);
    // Travel is only charged where the project has an account, and a lifetime
    // allowance establishes one without admitting anything neural.
    // §FS-rhei-budgets.5.2
    establish_account(&dir);

    // Spend the host ticket's travel first, so the assertion that the placed
    // ticket's bound is untouched is about a bound that has actually been used.
    let moved = run_into(
        &[
            "transition",
            "release",
            "--task",
            "ticket",
            "--from",
            "pending",
            "--to",
            "completed",
            "--no-callbacks",
            "--result",
            "the host ticket shipped",
        ],
        &dir,
    );
    assert_success(&moved);
    let host_identity = identities(&root.join("index.rhei.md"));
    assert_eq!(host_identity.len(), 1, "the host ticket has one identity: {host_identity:?}");

    let placed =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&placed);

    // The placed ticket's first move mints: a second, distinct identity.
    let first = run_into(
        &[
            "transition",
            "release",
            "--task",
            "coordinate",
            "--from",
            "review",
            "--to",
            "decide",
            "--no-callbacks",
        ],
        &dir,
    );
    assert_success(&first);

    let after = identities(&root.join("index.rhei.md"));
    assert_eq!(after.len(), 2, "two tickets, two identities: {after:?}");
    let uuids: Vec<&String> = after.iter().map(|(_, uuid)| uuid).collect();
    assert_ne!(uuids[0], uuids[1], "a placement at a free id mints a distinct identity");

    let journal = receipts(&root);
    assert_eq!(
        kinds(&journal, "identity").len(),
        2,
        "one `identity` receipt per ticket identity: {journal:?}"
    );

    // The placed ticket's bound is its own: the host's spending bought it
    // nothing and cost it nothing.
    let second = run_into(
        &[
            "transition",
            "release",
            "--task",
            "coordinate",
            "--from",
            "decide",
            "--to",
            "completed",
            "--no-callbacks",
            "--result",
            "the placed ticket finished on its own bound",
        ],
        &dir,
    );
    assert_success(&second);
}

// ---------------------------------------------------------------------------
// (1) delete a spent ticket and re-place at the same id
// ---------------------------------------------------------------------------

/// Delete a ticket that has spent its travel, then place a freshly rendered,
/// identity-free template at the same id. The binding outlives the document, so
/// the placed ticket adopts the deleted ticket's uuid and is halted on its first
/// move — a placement is not a door back to a fresh counter.
/// §FS-rhei-library.5 §FS-rhei-budgets.5.2 §REQ-bounded-neural-work.4
#[test]
fn deleting_a_spent_ticket_and_re_placing_it_adopts_the_old_identity() {
    let (dir, root) = host_workspace("identity-reuse");
    let template = write_review_template(&dir);
    // One ticket, because the id-collision refusal would otherwise fire on the
    // surviving sibling before the re-placement is reached. §FS-rhei-library.4
    fs::remove_file(template.join("tasks/002-record.md")).expect("drop the second ticket");
    establish_account(&dir);
    // One travel unit per lane, so one move spends a ticket. The template's own
    // profile carries it, because the second placement joins a machine that
    // already holds that profile. §FS-rhei-library.3.1
    write_fixture_file(
        &root,
        "states.yaml",
        &HOST_MACHINE.replace(
            "profiles:\n  host:\n    initial: pending\n",
            "profiles:\n  host:\n    initial: pending\n    transition_limit: 1\n",
        ),
    );
    write_fixture_file(
        &template,
        "states.yaml",
        &REVIEW_TEMPLATE_MACHINE.replace(
            "  review-loop:\n    initial: review\n",
            "  review-loop:\n    initial: review\n    transition_limit: 1\n",
        ),
    );

    let placed =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&placed);

    let spend = run_into(
        &[
            "transition",
            "release",
            "--task",
            "coordinate",
            "--from",
            "review",
            "--to",
            "decide",
            "--no-callbacks",
        ],
        &dir,
    );
    assert_success(&spend);
    let spent_uuid = identities(&root.join("index.rhei.md"))
        .into_iter()
        .find(|(id, _)| id == "coordinate")
        .map(|(_, uuid)| uuid)
        .expect("the spent ticket has an identity");

    // Delete the ticket's document and its frontmatter entry: the plan now
    // looks, to a reader, as if `coordinate` had never existed.
    fs::remove_file(root.join("tasks/002-coordinate.md")).expect("delete the spent ticket");
    let index = read(&root.join("index.rhei.md"));
    let kept: Vec<&str> = index
        .lines()
        .filter(|line| !line.trim_start().starts_with("budgetTicketId:"))
        .filter(|line| line.trim() != "coordinate:")
        .collect();
    let stripped = format!("{}\n", kept.join("\n"));
    write_fixture_file(&root, "index.rhei.md", &stripped);

    // Place the same template again at the same id.
    let again =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&again);

    let halted = run_into(
        &[
            "transition",
            "release",
            "--task",
            "coordinate",
            "--from",
            "review",
            "--to",
            "decide",
            "--no-callbacks",
        ],
        &dir,
    );
    assert!(
        !halted.status.success(),
        "a ticket re-placed at a bound id is halted on its first move; got:\nstdout:\n{}\nstderr:\n{}",
        halted.stdout,
        halted.stderr
    );
    let combined = format!("{}{}", halted.stdout, halted.stderr);
    assert!(
        combined.contains("ticket travel"),
        "the halt should name ticket travel; got:\n{combined}"
    );
    // The uuid is read from the ledger rather than from the plan, because a
    // halted move rewrites nothing: the binding is what the account settled,
    // and the document the placement wrote never got one of its own.

    // One `identity` receipt for the id, still naming the deleted ticket's
    // uuid, is the whole claim — a placement that had minted afresh would have
    // written a second. §FS-rhei-budgets.5.2 §REQ-bounded-neural-work.4
    let journal = receipts(&root);
    let bound: Vec<&serde_json::Value> = kinds(&journal, "identity")
        .into_iter()
        .filter(|receipt| receipt["payload"]["display_id"] == "release.coordinate")
        .collect();
    assert_eq!(bound.len(), 1, "the re-placement minted no identity of its own: {bound:?}");
    let adopted = bound[0]["payload"]["ticket_identity"]
        .as_str()
        .expect("an identity receipt names the ticket identity it settled");
    assert!(
        adopted.ends_with(&spent_uuid),
        "the binding outlives the document, so the re-placed ticket carries the deleted ticket's \
         uuid {spent_uuid}; got {adopted}"
    );
}

/// Put the project on a lifetime invocation allowance, which is what gives it a
/// budget account: travel is charged against an account, and a rhei that never
/// had one is not charged at all — so an identity assertion over a plan with no
/// account would pass by there being nothing to assert.
/// §FS-rhei-budgets.5.2 §REQ-bounded-neural-work.4
fn establish_account(dir: &Path) {
    let opened = run_into(
        &["budget", "init", "--invocations", "50", "--reason", "identity fixture", "release"],
        dir,
    );
    assert_success(&opened);
}
