//! A ticket's identity survives a relocated root and a rewritten plan file —
//! and belongs to one live ticket while it does.
//!
//! The binding is by source path **and** content hash, so several live sources
//! are tolerated only while their bytes agree. Agreeing bytes are what tell a
//! relocated document from a conflicting one, and say nothing about how many
//! live tickets claim the identity: that is what makes a *moved* ticket keep
//! its travel and a copy of one refused it.
//! §FS-rhei-budgets.5.2 §FS-rhei-budgets.5.2.1

use super::bounds::row;
use super::journal::{Audit, Journal};
use super::types::BudgetError;
use super::Result;
use crate::metadata::{budget_identity_claimants, parse_metadata_file};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

impl Journal {
    pub fn identities(&self) -> &BTreeMap<String, Value> {
        &self.state.identities
    }

    /// Whether this ticket identity is durably installed and may be reserved
    /// against. §FS-rhei-budgets.6.1
    pub fn identity_installed(&self, ticket: &str) -> bool {
        self.state.identities.get(ticket).is_some_and(|binding| binding["status"] == "installed")
    }

    /// Every registered copy's path, not just the latest root. The driver locks
    /// these before reading any binding. §AR-neural-admission.3
    pub fn identity_sources(&self) -> Result<BTreeSet<PathBuf>> {
        let mut paths = BTreeSet::new();
        for binding in self.state.identities.values() {
            paths.extend(sources(binding)?);
        }
        Ok(paths)
    }

    /// The ticket uuid the ledger already binds to this source path and display
    /// id, if it has one.
    ///
    /// This is what a caller asks before minting. A ticket whose plan write was
    /// lost after its receipts were appended still has its binding here, so it
    /// adopts the identity it already spent against instead of being handed a
    /// fresh one — and with it a second travel bound. Stripping
    /// `budgetTicketId` from a plan by hand therefore restores that history
    /// rather than resetting it, which is the same answer [§FS-rhei-budgets.5.2](../../../../docs/functional-spec/rhei-budgets.spec.md)
    /// gives a moved ticket.
    ///
    /// The display id is part of the key rather than decoration: one metadata
    /// file holds every task of a rhei, so a binding matched on the path alone
    /// would hand one ticket's travel to its sibling.
    ///
    /// Asked unconditionally, and on purpose: the document a renumber leaves —
    /// one keyless ticket beside one that holds the uuid — is the same document
    /// a deleted key leaves beside a copy that holds it, and declining to
    /// answer the first mints a second bound for the ticket that already spent
    /// against the first. §FS-rhei-budgets.5.2.1
    pub fn bound_ticket(&self, display: &str, source: &Path) -> Result<Option<String>> {
        let source = match std::fs::canonicalize(source) {
            Ok(source) => source,
            // Nothing can be bound to a path that is not there.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let prefix = format!("ticket:{}:", self.project_id.trim_start_matches("panta:"));
        for (ticket, binding) in &self.state.identities {
            if binding["display_id"] != display || !sources(binding)?.contains(&source) {
                continue;
            }
            if let Some(id) = ticket.strip_prefix(&prefix) {
                return Ok(Some(id.to_string()));
            }
        }
        Ok(None)
    }

    /// Validate every identity, including ones absent from the selected root.
    /// §FS-rhei-budgets.5.2
    pub fn validate_identity_sources(&self) -> Result<()> {
        for binding in self.state.identities.values() {
            let mut content = None;
            for source in sources(binding)? {
                if let Some(bytes) = read_live_source(&source)? {
                    if content.as_ref().is_some_and(|old| old != &bytes) {
                        return Err(conflict());
                    }
                    content = Some(bytes);
                }
            }
        }
        Ok(())
    }

    /// Record where this ticket identity lives, and say so when the display id
    /// it is counted against moves.
    ///
    /// A second live source must contain the same bytes; a missing old source
    /// permits a move. Rebinding the same path is idempotent, which is what
    /// lets an ordinary plan rewrite — a state change, a visit counter, `rhei
    /// reset` restoring the authored state — leave the identity alone.
    ///
    /// The display id may only move to one the document no longer claims the
    /// identity under: while more than one live ticket claims the uuid, the
    /// admission is refused rather than rebound, because one travel bound
    /// covers one ticket and two tickets drawing on one binding is one bound
    /// covering two. A move the rule permits is returned rather than printed —
    /// the core appends the receipt and says what it did, and the surface that
    /// has a warning channel is the one that writes to it.
    /// §FS-rhei-budgets.5.2 §FS-rhei-budgets.5.2.1
    pub fn bind_ticket(
        &mut self,
        ticket: &str,
        display: &str,
        source: &Path,
        audit: &Audit,
    ) -> Result<Option<IdentityMove>> {
        let prefix = format!("ticket:{}:", self.project_id.trim_start_matches("panta:"));
        let id = ticket
            .strip_prefix(&prefix)
            .ok_or_else(|| BudgetError::corrupt("ticket belongs to another account"))?;
        super::types::uuid(id)?;
        let source = std::fs::canonicalize(source)?;
        let bytes = crate::source::read_to_string(&source)?;
        let mut live = BTreeSet::from([source.clone()]);
        let mut held = None;
        if let Some(previous) = self.state.identities.get(ticket) {
            let previous_sources = sources(previous)?;
            for old in &previous_sources {
                if let Some(existing) = read_live_source(old)? {
                    if existing != bytes {
                        return Err(conflict());
                    }
                    live.insert(old.clone());
                }
            }
            if previous_sources == live && previous["display_id"] == display {
                return Ok(None);
            }
            // Only a changed display id is in question. This branch is also
            // where a relocated root arrives under the display id it always
            // had, which §FS-rhei-budgets.5.2 tolerates on purpose.
            if previous["display_id"] != display {
                held = Some(super::replay::string(previous, "display_id")?);
            }
        }
        let moved = match held {
            Some(held) => Some(claim_moving(id, held, display, &live, &bytes)?),
            None => None,
        };
        self.append(
            "identity",
            json!({"ticket_identity": ticket, "display_id": display,
            "source_path": source, "source_paths": live,
            "source_hash": super::journal::digest(bytes.as_bytes()),
            "status": "installed"}),
            audit,
        )?;
        Ok(moved)
    }
}

/// A binding whose display id moved: lawful, and no longer silent.
///
/// A renumbered task and a renamed plan file are both this, and neither can be
/// told from the other in the document — which is why the move is reported
/// rather than refused. A writer that drops it says nothing about a binding that
/// moved durably, so every writer binds it by name and reports it before
/// anything else about the edge can refuse it. The `#[must_use]` below is a
/// reminder rather than a guard: `bind_ticket` hands the move back inside an
/// `Option`, and `unused_must_use` does not look through one.
/// §FS-rhei-budgets.5.2.1
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a binding that moved is reported on the warning channel"]
pub struct IdentityMove {
    /// The ticket uuid, as the account's own receipts spell it.
    pub identity: String,
    /// The display id the account counted this uuid against until now.
    pub from: String,
    /// The display id it is counted against from here.
    pub to: String,
}

impl IdentityMove {
    /// The warning channel's words for a move, in the shape
    /// §FS-rhei-budgets.5.2.1 fixes.
    ///
    /// The continuation is indented to the message's own column rather than to
    /// wherever the source happens to wrap: this line is printed plain rather
    /// than through a reporter, so what is built here is exactly what is read.
    pub fn warning(&self) -> String {
        let (identity, to, from) = (&self.identity, &self.to, &self.from);
        // `counted` aligns under `travel`, which is the width of the label.
        let indent = " ".repeat("warning: ".len());
        format!(
            "warning: travel for {identity} now counts against '{to}'; it was\n\
             {indent}counted against '{from}', which this plan no longer has"
        )
    }
}

/// Whether the display id may move, given how many live tickets still claim the
/// uuid: exactly one or none is a move, more than one is the copy this rule
/// exists to refuse. §FS-rhei-budgets.5.2.1
fn claim_moving(
    id: &str,
    held: &str,
    display: &str,
    live: &BTreeSet<PathBuf>,
    bytes: &str,
) -> Result<IdentityMove> {
    // Every live source holds the same bytes — a disagreement is the conflict
    // refused above — so the count reads what is already in hand rather than
    // the files again. The path still decides the parse, because which document
    // form a name asks for is what tells where a ticket's metadata lives.
    let mut claimed = 0;
    let mut claiming: Vec<&PathBuf> = Vec::new();
    for path in live {
        let count = claimants(path, bytes, id);
        if count > 0 {
            claiming.push(path);
        }
        claimed += count;
    }
    if claimed > 1 {
        return Err(claimed_by_two(id, held, display, &claiming));
    }
    Ok(IdentityMove { identity: id.to_string(), from: held.to_string(), to: display.to_string() })
}

/// How many tickets of one live document claim `id` as their budget identity.
///
/// A document that will not parse is passed over rather than counted as a
/// claim, exactly as a missing one is: an unparseable sibling must not be able
/// to refuse an admission that has nothing to do with it.
/// §FS-rhei-budgets.5.2.1
fn claimants(path: &Path, bytes: &str, id: &str) -> usize {
    match parse_metadata_file(path, bytes) {
        Ok(metadata) => budget_identity_claimants(metadata.as_ref(), id),
        Err(_) => 0,
    }
}

/// Two live tickets claiming one identity, in the plain `error:` shape of
/// §FS-rhei-budgets.8 — no bound is what stopped this — with both display ids,
/// the uuid, the file they are live in, and one remedy.
///
/// The remedy names neither of them as the copy. The refused ticket is usually
/// the copy and sometimes the ticket that earned the history — a renumber whose
/// freed display id was authored over before it moved — and which it is, is not
/// in the document. §FS-rhei-budgets.5.2.1 §FS-rhei-errors.1
fn claimed_by_two(id: &str, held: &str, display: &str, claiming: &[&PathBuf]) -> BudgetError {
    let lines = [
        format!("ticket '{display}' claims a budget identity the account holds for '{held}'"),
        row("identity:", id),
        row("claimed by:", &format!("{held} and {display}, {}", live_in(claiming))),
        row("why:", "one travel bound covers one ticket, and these two would"),
        row("", "draw on one"),
        row("to fix it:", "give whichever of these did not earn this history a fresh"),
        row("", "`budgetTicketId` in its file; the account counts it"),
        row("", &format!("against '{held}'")),
    ];
    BudgetError::new("identity_claimed", lines.join("\n"))
}

/// Where the claims are live: one document by name, because that is what a
/// person greps for, and several by path, because a bare name would not locate
/// either of them. §FS-rhei-budgets.5.2.1
fn live_in(claiming: &[&PathBuf]) -> String {
    match claiming {
        [one] => {
            format!("both live in {}", one.file_name().unwrap_or(one.as_os_str()).to_string_lossy())
        }
        several => format!(
            "live in {}",
            several.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(" and ")
        ),
    }
}

/// Earlier receipts accumulated bindings too; replay must not discard them.
/// §FS-rhei-budgets.5.2
pub(crate) fn replay_binding(previous: Option<&Value>, payload: &Value) -> Result<Value> {
    let mut binding = payload.clone();
    let mut paths = sources(payload)?;
    if payload.get("source_paths").is_none() {
        if let Some(previous) = previous {
            paths.extend(sources(previous)?);
        }
    }
    binding["source_paths"] = serde_json::to_value(paths)?;
    Ok(binding)
}

fn sources(binding: &Value) -> Result<BTreeSet<PathBuf>> {
    let current = super::replay::string(binding, "source_path")?;
    let mut paths = match binding.get("source_paths") {
        Some(value) => serde_json::from_value::<BTreeSet<PathBuf>>(value.clone())?,
        None => BTreeSet::new(),
    };
    paths.insert(current.into());
    if paths.iter().any(|path| !path.is_absolute()) {
        return Err(BudgetError::corrupt("identity source path must be absolute"));
    }
    Ok(paths)
}

fn read_live_source(path: &Path) -> Result<Option<String>> {
    match crate::source::read_to_string(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn conflict() -> BudgetError {
    BudgetError::new("identity_conflict", "ticket identity has conflicting live content")
}
