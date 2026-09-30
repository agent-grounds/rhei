// What `rhei budget show` prints about an account it cannot verify, and the
// refusal it ends in.
//
// Its own part because it is the one budget surface that has to be *readable*
// in the state a run was refused for: the command a refusal names may not
// itself fail to open the account it was asked to describe. The report is
// therefore prose on stdout with the remedies spelled as runnable commands,
// and the refusal beneath it carries the same facts as named members for a
// harness.

// §AR-source-file-size.3 §FS-rhei-budgets.10 §FS-rhei-errors.5

use rhei_core::budget::{Damage, Diagnosis};

/// A refusal whose subject the caller has to *read* rather than only act on.
///
/// `message` and `help` keep exactly their meaning; the detail members are
/// what the prose already said, in the shape a reader does not have to parse
/// it out of. Only `--format json` renders them — the text path already
/// printed the report. §FS-rhei-errors.5
#[derive(Debug)]
struct DetailedRefusal {
    message: String,
    help: String,
    details: serde_json::Map<String, serde_json::Value>,
}

impl std::fmt::Display for DetailedRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DetailedRefusal {}

impl DetailedRefusal {
    fn members(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.details
    }
}

impl miette::Diagnostic for DetailedRefusal {
    fn help(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        Some(Box::new(&self.help))
    }
}

/// Report the damaged account, then refuse.
///
/// The report goes to stdout under `--format text` and nowhere at all under
/// `--format json`, where a reader parses one shape on stderr and must not
/// find prose beside it. Nothing here writes to the account.
/// §FS-rhei-budgets.10
fn budget_report_damaged(diagnosis: &Diagnosis, format: BudgetFormat) -> miette::Report {
    if matches!(format, BudgetFormat::Text) {
        for line in budget_damaged_lines(diagnosis) {
            println!("{line}");
        }
    }
    miette::Report::new(DetailedRefusal {
        message: diagnosis.headline(),
        help: budget_damaged_help(diagnosis),
        details: budget_damaged_details(diagnosis),
    })
}

/// The next action, by sub-case.
///
/// Where a journal is present it **is** this project's and its tail is what
/// was lost, so the next action is the restore. Where it is wholly absent the
/// tool does not know whose history this is, so the next action is the one
/// command that lets the operator say — and never the copy, which succeeds and
/// charges a new project for the spend of the one that used the path before
/// it. §FS-rhei-budgets.5.4
fn budget_damaged_help(diagnosis: &Diagnosis) -> String {
    match diagnosis.forget_command() {
        Some(forget) => format!("if the path was reused, retire that record with: {forget}"),
        None => format!("restore this project's journal with: {}", diagnosis.restore_command()),
    }
}

/// The same facts as named members, for a harness that reads a damaged
/// account. The vocabulary is the closed one of §FS-rhei-budgets.10: `health`
/// is `damaged` here and `verified` nowhere else, and `damage` is one of the
/// three sub-cases.
///
/// `project_id` and `account` are the members a verified report already
/// carries, with the meanings it gives them — the identity and the account
/// **directory**, absent or not. A key that named a directory in one state and
/// a bare uuid in the other would be two shapes under one name, which is the
/// one thing §FS-rhei-budgets.10 promises a reader against. §FS-rhei-errors.5
fn budget_damaged_details(diagnosis: &Diagnosis) -> serde_json::Map<String, serde_json::Value> {
    let mut details = serde_json::Map::new();
    details.insert("health".into(), "damaged".into());
    details.insert("damage".into(), diagnosis.damage.as_str().into());
    details.insert("project_id".into(), diagnosis.project_id().into());
    details.insert("project_root".into(), serde_json::json!(diagnosis.root));
    details.insert("account".into(), serde_json::json!(diagnosis.directory()));
    details.insert("journal".into(), serde_json::json!(diagnosis.journal));
    details.insert("witness".into(), serde_json::json!(diagnosis.witness));
    details.insert("receipts".into(), diagnosis.history.receipts.into());
    details.insert("invocations".into(), diagnosis.history.invocations.into());
    details.insert("restore".into(), diagnosis.restore_command().into());
    if let Some(forget) = diagnosis.forget_command() {
        details.insert("forget".into(), forget.into());
    }
    details
}

/// The report itself: what was found, what the recorded history holds, and one
/// runnable command per remedy the sub-case admits.
///
/// Where the journal is wholly absent both readings are stated and neither is
/// chosen, because the distinguishing fact is the operator's intent and nothing
/// on disk carries it. §FS-rhei-budgets.5.4
fn budget_damaged_lines(diagnosis: &Diagnosis) -> Vec<String> {
    let mut lines = vec![
        format!("Project: {} (panta:{})", diagnosis.root.display(), diagnosis.uuid),
        format!("Account: damaged [{}] — {}", diagnosis.damage, diagnosis.headline()),
        match diagnosis.damage {
            Damage::JournalAbsent => format!("Journal: {} (absent)", diagnosis.journal.display()),
            _ => format!("Journal: {}", diagnosis.journal.display()),
        },
        format!("Recorded history: {}", diagnosis.witness.display()),
        format!("  {}", diagnosis.history.summary()),
        String::new(),
    ];
    let Some(forget) = diagnosis.forget_command() else {
        lines.push(
            "This journal is this project's own and its tail is what was lost — restore it:"
                .into(),
        );
        lines.push(format!("  {}", diagnosis.restore_command()));
        return lines;
    };
    lines.push("Rhei cannot tell which of two things happened, so it does not choose:".into());
    lines.push(String::new());
    lines.push("  this project's journal was lost — restore it:".into());
    lines.push(format!("    {}", diagnosis.restore_command()));
    lines.push(String::new());
    lines.push("  the path was previously held by a different project — retire that record:".into());
    lines.push(format!("    {forget}"));
    lines
}
