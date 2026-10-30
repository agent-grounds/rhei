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

use rhei_core::budget::Diagnosis;

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
///
/// `target` is the project as the operator spelled it on the command line, and
/// it is carried in rather than taken from the diagnosis because the diagnosis
/// holds the **resolved** root — the key the witness index is by. One of the two
/// belongs on the `Project:` line and under `project_root`, and it is the
/// operator's spelling, which is what a verified `show` has always printed
/// there. §FS-rhei-budgets.10
fn budget_report_damaged(
    diagnosis: &Diagnosis,
    target: &Path,
    format: BudgetFormat,
) -> miette::Report {
    if matches!(format, BudgetFormat::Text) {
        for line in budget_damaged_lines(diagnosis, target) {
            println!("{line}");
        }
    }
    miette::Report::new(DetailedRefusal {
        message: diagnosis.headline(),
        help: budget_damaged_help(diagnosis),
        details: budget_damaged_details(diagnosis, target),
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
///
/// A copy is offered `forget` for itself and moving the holder for the other
/// reading, a tail from another root the set-aside, and a witness that ends
/// mid-receipt the trim, with `forget` beside it where there is no journal; none
/// is offered the copy of the witness. §FS-rhei-budgets.5.4.1
/// §FS-rhei-budgets.5.4.2 §FS-rhei-budgets.5.4.3
fn budget_damaged_help(diagnosis: &Diagnosis) -> String {
    if let Some(set_aside) = diagnosis.set_aside_command() {
        return format!("set the witness aside, keeping every byte, with: {set_aside}");
    }
    if let Some(trim) = diagnosis.trim_command() {
        let forget = diagnosis
            .forget_command()
            .map(|forget| format!("; if the path was reused, retire that record with: {forget}"))
            .unwrap_or_default();
        return format!(
            "keep the witness and cut it back to its last whole receipt with: {trim}{forget}"
        );
    }
    match (diagnosis.forget_command(), &diagnosis.held_by) {
        (Some(forget), Some(holder)) => format!(
            "if this is a copy, give it an account of its own with: {forget}; if it is meant to \
             replace {}, move or delete that project first",
            holder.display()
        ),
        (Some(forget), None) => format!("if the path was reused, retire that record with: {forget}"),
        (None, _) => format!(
            "restore this project's journal with: {}",
            diagnosis.restore_command().unwrap_or_default()
        ),
    }
}

/// The same facts as named members, for a harness that reads a damaged
/// account. The vocabulary is the closed one of §FS-rhei-budgets.10: `health`
/// is `damaged` here and `verified` nowhere else, and `damage` is one of the
/// six sub-cases. `held_by` and `foreign_root` name the other root under the
/// two sub-cases that have one, `restore` is absent where the witness is not
/// this root's to restore from, and `trim` is present under `witness_torn` and
/// nowhere else, because a cut of the witness under `restore`'s name would give
/// one member two opposite meanings.
///
/// `project_id`, `project_root` and `account` are the members a verified report
/// already carries, with the meanings it gives them — the identity, the target
/// as the operator spelled it, and the account **directory**, absent or not. A
/// key that named a directory in one state and a bare uuid in the other, or a
/// root resolved in one and given in the other, would be two shapes under one
/// name, which is the one thing §FS-rhei-budgets.10 promises a reader against.
/// §FS-rhei-errors.5
fn budget_damaged_details(
    diagnosis: &Diagnosis,
    target: &Path,
) -> serde_json::Map<String, serde_json::Value> {
    let mut details = serde_json::Map::new();
    details.insert("health".into(), "damaged".into());
    details.insert("damage".into(), diagnosis.damage.as_str().into());
    details.insert("project_id".into(), diagnosis.project_id().into());
    details.insert("project_root".into(), serde_json::json!(target));
    details.insert("account".into(), serde_json::json!(diagnosis.directory()));
    details.insert("journal".into(), serde_json::json!(diagnosis.journal));
    details.insert("witness".into(), serde_json::json!(diagnosis.witness));
    details.insert("receipts".into(), diagnosis.history.receipts.into());
    details.insert("invocations".into(), diagnosis.history.invocations.into());
    if let Some(restore) = diagnosis.restore_command() {
        details.insert("restore".into(), restore.into());
    }
    if let Some(forget) = diagnosis.forget_command() {
        details.insert("forget".into(), forget.into());
    }
    if let Some(trim) = diagnosis.trim_command() {
        details.insert("trim".into(), trim.into());
    }
    if let Some(holder) = &diagnosis.held_by {
        details.insert("held_by".into(), serde_json::json!(holder));
    }
    if let Some(foreign) = &diagnosis.foreign {
        details.insert("foreign_root".into(), serde_json::json!(foreign.root));
    }
    details
}

/// The report itself: what was found, what the recorded history holds, and one
/// runnable command per remedy the sub-case admits.
///
/// Where the journal is wholly absent both readings are stated and neither is
/// chosen, because the distinguishing fact is the operator's intent and nothing
/// on disk carries it. §FS-rhei-budgets.5.4
fn budget_damaged_lines(diagnosis: &Diagnosis, target: &Path) -> Vec<String> {
    let mut lines = vec![
        format!("Project: {} (panta:{})", target.display(), diagnosis.uuid),
        format!("Account: damaged [{}] — {}", diagnosis.damage, diagnosis.headline()),
        match diagnosis.journal_present {
            false => format!("Journal: {} (absent)", diagnosis.journal.display()),
            true => format!("Journal: {}", diagnosis.journal.display()),
        },
        format!("Recorded history: {}", diagnosis.witness.display()),
        format!("  {}", diagnosis.history.summary()),
        String::new(),
    ];
    if let Some(holder) = &diagnosis.held_by {
        // §FS-rhei-budgets.5.4.1
        lines.push(format!("Held by: {}, which still holds this account", holder.display()));
        lines.push(String::new());
        lines.push("  if this project is a copy, give it an account of its own:".into());
        lines.push(format!("    {}", diagnosis.forget_command().unwrap_or_default()));
        lines.push(String::new());
        lines.push(format!(
            "  if it is meant to replace {}, move or delete that project first",
            holder.display()
        ));
        return lines;
    }
    if let (Some(foreign), Some(set_aside)) = (&diagnosis.foreign, diagnosis.set_aside_command()) {
        // §FS-rhei-budgets.5.4.2
        let receipts = match foreign.receipts {
            1 => "1 receipt".to_string(),
            n => format!("{n} receipts"),
        };
        lines.push(format!(
            "The {receipts} past this journal were written from {}, not from this project; \
             copying the witness back would import them.",
            foreign.root.display()
        ));
        lines.push("Set the witness aside, keeping every byte; the next run adopts this journal:".into());
        lines.push(format!("  {set_aside}"));
        return lines;
    }
    if let Some(trim) = diagnosis.trim_command() {
        return budget_torn_lines(diagnosis, &trim, lines);
    }
    let Some(forget) = diagnosis.forget_command() else {
        lines.push(
            "This journal is this project's own and its tail is what was lost — restore it:"
                .into(),
        );
        lines.push(format!("  {}", diagnosis.restore_command().unwrap_or_default()));
        return lines;
    };
    lines.push("Rhei cannot tell which of two things happened, so it does not choose:".into());
    lines.push(String::new());
    lines.push("  this project's journal was lost — restore it:".into());
    lines.push(format!("    {}", diagnosis.restore_command().unwrap_or_default()));
    lines.push(String::new());
    lines.push("  the path was previously held by a different project — retire that record:".into());
    lines.push(format!("    {forget}"));
    lines
}

/// A witness that ends mid-receipt: the trim, never the copy, and what comes
/// after it. The account is read again once the witness is cut back, and is
/// then whatever its whole receipts make it — which may offer the copy, safe by
/// then. Where the journal is wholly absent, `forget` stands beside the trim.
/// §FS-rhei-budgets.5.4.3
fn budget_torn_lines(diagnosis: &Diagnosis, trim: &str, mut lines: Vec<String>) -> Vec<String> {
    lines.push(format!(
        "The witness ends mid-receipt, {}: the fragment of an append that was cut short. \
         Copying it anywhere would carry the fragment along.",
        diagnosis.fragment_phrase().unwrap_or_default()
    ));
    lines.push("Keep every byte and cut the witness back to its last whole receipt:".into());
    lines.push(format!("  {trim}"));
    lines.push(String::new());
    lines.push(
        "then run `rhei budget show` again: the account is read again and answered as what its \
         whole receipts make it."
            .into(),
    );
    if let Some(forget) = diagnosis.forget_command() {
        lines.push(String::new());
        lines.push(
            "If the path was previously held by a different project, retire that record instead:"
                .into(),
        );
        lines.push(format!("  {forget}"));
    }
    lines
}
