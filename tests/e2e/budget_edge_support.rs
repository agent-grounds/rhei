//! The fixture world the scenarios that drive a plan **by hand** share: a
//! ping-pong machine with no declared outputs, the ticket identity the plan
//! carries, and the account's receipts in the order they were appended.
//!
//! [`super::budget_support`] is the same thing for the scenarios `rhei run`
//! drives. These three are separate because a hand edge is held to a state's
//! declared outputs where an agent would have written them, so the machine
//! differs — and because what a hand-driven scenario reads afterwards is the
//! document and the journal rather than a spawn log.

use std::fs;
use std::path::Path;

/// A ping-pong that declares no output artifacts.
///
/// The scenarios that take every edge by hand need it, because `rhei
/// transition` holds a state's declared outputs against the move — no agent
/// runs there, so nothing would ever write one. [`super::budget_support::PING_PONG_MACHINE`] declares
/// them because the scenarios it serves let `rhei run` drive the loop.
/// §FS-rhei-states.3.3
pub(super) const BARE_PING_PONG_MACHINE: &str = r#"name: budget-identity-ping-pong
version: 1
states:
  work:
    initial: true
    description: Do a round of work
    agent: mock
    agent_timeout: 30s
  review:
    description: Send it back for another round
    agent: mock
    agent_timeout: 30s
  cancelled:
    description: Stop
    final: true
transitions:
  - { from: work, to: review, description: Round done }
  - { from: review, to: work, description: Another round }
  - { from: work, to: cancelled, description: Stop }
  - { from: review, to: cancelled, description: Stop }
"#;

/// The `budgetTicketId` the plan's frontmatter carries for a task, if any.
///
/// Read line by line rather than through a YAML parse because the metadata keys
/// are task *numbers*, which a parser hands back as integers while every caller
/// here has a display id in hand. §FS-rhei-budgets.5.2
pub(super) fn budget_identity(plan: &Path, task: &str) -> Option<String> {
    let text = fs::read_to_string(plan).expect("read the plan");
    let mut under: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(id) = trimmed.strip_suffix(':') {
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                under = Some(id);
            }
        }
        if under == Some(task) {
            if let Some(uuid) = trimmed.strip_prefix("budgetTicketId:") {
                return Some(uuid.trim().to_owned());
            }
        }
    }
    None
}

/// Every receipt in the project's account, in the order it was appended.
///
/// The account directory is read rather than named, because its uuid is minted
/// at establishment and no test is told what it is. §FS-rhei-budgets.5.2
pub(super) fn account_journal(root: &Path) -> Vec<serde_json::Value> {
    let accounts = root.join(".agent-grounds/rhei/budgets");
    let mut journal = String::new();
    for account in fs::read_dir(&accounts).expect("the project has a budget account") {
        let path = account.expect("an account directory").path().join("journal.jsonl");
        journal.push_str(&fs::read_to_string(&path).unwrap_or_default());
    }
    journal
        .lines()
        .map(|line| serde_json::from_str(line).expect("every journal line is one JSON receipt"))
        .collect()
}
