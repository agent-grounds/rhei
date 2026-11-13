//! When a qualified id matches no loose rhei, or more than one, the refusal says
//! so in terms of the id and offers both forms that work (#676).
//! §FS-rhei-panta.6.7 §FS-rhei-complete.2.1

use super::loose_rhei_ticket_target_support::Loose;
use super::new_tests::flattened_output;

fn refusal(loose: &Loose, args: &[&str]) -> String {
    let result = loose.rhei(args);
    let said = flattened_output(&result);
    assert_eq!(result.status.code(), Some(1), "`rhei {}` should refuse:\n{said}", args.join(" "));
    said
}

/// `venice.1` matches no rhei here: the error says no rhei has the id `venice`,
/// lists the candidates, and offers the qualified and the plan form.
/// §FS-rhei-panta.6.7
#[test]
fn an_unmatched_prefix_names_the_id_and_both_forms() {
    let loose = Loose::new("loose-refuse-venice");
    let said = refusal(&loose, &["release", "venice.1"]);
    assert!(said.contains("no rhei") && said.contains("`venice`"), "{said}");
    assert!(said.contains("italy.rhei.md") && said.contains("rome.rhei.md"), "{said}");
    assert!(
        said.contains("rhei release italy.1") || said.contains("rhei release rome.1"),
        "the qualified form should be offered:\n{said}"
    );
    assert!(said.contains(".rhei.md --task 1"), "the plan form should be offered:\n{said}");
}

/// `review.1` may be subtask 1 of a ticket `review`, so the error must not
/// claim the prefix names a rhei. §FS-rhei-panta.6.7
#[test]
fn an_unmatched_prefix_is_not_called_a_rhei() {
    let loose = Loose::new("loose-refuse-review");
    let said = refusal(&loose, &["show", "review.1"]);
    assert!(said.contains("`review`"), "the error should name the id it compared:\n{said}");
    assert!(!said.contains("names the rhei"), "review.1 need not name a rhei:\n{said}");
}

/// An entry with no usable id is still listed among the candidates.
/// §FS-rhei-panta.6.7
#[test]
fn an_unmatched_prefix_still_lists_entries_with_no_usable_id() {
    let loose = Loose::new("loose-refuse-underivable")
        .with("basin.rhei.md", "# Rhei: Basin\n\n## Tasks\n")
        .with("not.valid.rhei.md", "# Rhei: Not valid\n\n## Tasks\n");
    let said = refusal(&loose, &["show", "venice.1"]);
    assert!(said.contains("`venice`"), "{said}");
    assert!(said.contains("basin.rhei.md") && said.contains("not.valid.rhei.md"), "{said}");
}

/// Two entries with one id: the error says the id names two rheis and names
/// both entries. §FS-rhei-panta.6.7
#[test]
fn a_prefix_two_rheis_share_names_both() {
    let loose =
        Loose::new("loose-refuse-duplicate").with("italy/index.rhei.md", "# Rhei: Italy two\n");
    let said = refusal(&loose, &["show", "italy.1"]);
    assert!(said.contains("two rheis"), "the error should say `italy` names two rheis:\n{said}");
    assert!(said.contains("italy.rhei.md") && said.contains("italy"), "{said}");
}

/// A bare number keeps today's refusal, and a ticket command's help adds that
/// the ticket may be named by its rhei. §FS-rhei-panta.6.7
#[test]
fn a_bare_number_is_told_to_name_the_rhei() {
    let loose = Loose::new("loose-refuse-bare");
    let said = refusal(&loose, &["release", "1"]);
    assert!(said.contains("so there is no single plan to pick"), "{said}");
    assert!(said.contains("by its rhei"), "the help should offer the qualified form:\n{said}");
}
