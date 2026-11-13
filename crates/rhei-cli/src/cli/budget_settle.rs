// The settle that ends a visit: what each invocation actually cost, handed
// from the accounting record to the claim, and written as the visit's last
// receipt — with what an unpriced invocation was and which book could not
// price it, so the journal alone names both.
//
// Its own part because the admission next door decides whether a visit may
// start, and this decides what it owed once it did; the two meet only at the
// claim map.

// §AR-source-file-size.3 §FS-rhei-budgets.6.2 §FS-rhei-budgets.8.1

/// One invocation's cost as the settle will write it. §FS-rhei-budgets.6.2
#[derive(Clone, Debug)]
struct Measurement {
    amount_micro: u64,
    /// What the accounting record said it was denominated in, and `None` where
    /// the record said nothing — a pricing status of `not-applicable` carries
    /// no currency. Guessing one here would put a foreign receipt to an
    /// account that holds exactly one currency, which the ledger refuses; the
    /// settle falls back to the account's own instead. §FS-rhei-budgets.5.5
    currency: Option<String>,
    basis: SpendBasis,
    /// What an unpriced invocation was and which book could not price it, so
    /// its receipt names both. `None` on any other basis. §FS-rhei-budgets.6.2
    pricing_context: Option<rhei_core::budget::PricingContext>,
}

/// Hand the settle what this invocation actually cost.
///
/// Called where the accounting record is already durable, on whichever thread
/// wrote it: the claim map is process-global and keyed by ticket, so the
/// sequential and the parallel path reach it the same way. It is stored on
/// `arm`, the reservation of the invocation that wrote it, never on a sibling's.
/// §FS-rhei-budgets.6.2.1 `None` is the
/// class with no accounting extractor at all — no record is ever written for
/// it — and it is deliberately *not* recorded here, so the settle charges it
/// the reserve as `unmeasurable` rather than as nothing.
/// §FS-rhei-budgets.6.2 §FS-rhei-cost-accounting.3.2
fn budget_record_spend(
    task_id_str: &str,
    arm: Option<&str>,
    usage: Option<&rhei_tui::UsageSummary>,
    book: &PriceBook,
    origin: &PriceBookOrigin,
) {
    let (Some(arm), Some(usage)) = (arm, usage) else { return };
    // A record whose pricing is `unpriced` or `not-applicable` carries no
    // amount, and a day that read `$0.00` for it would read healthy hardest
    // for the models nobody has written a price for yet. The currency is
    // carried as the record gave it, `None` included: a `not-applicable`
    // record names none, and the settle knows the account's own.
    let (amount_micro, basis) = match usage.cost_micro.or(usage.priced_cost_micro) {
        Some(amount_micro) => (amount_micro, SpendBasis::Measured),
        None => (built_in::SPEND_RESERVE, SpendBasis::Unpriced),
    };
    let pricing_context = matches!(basis, SpendBasis::Unpriced).then(|| {
        unpriced_pricing_context(usage.provider.as_deref(), usage.model.as_deref(), book, origin)
    });
    let measurement =
        Measurement { amount_micro, currency: usage.currency.clone(), basis, pricing_context };
    with_claims(|claims| {
        if let Some(held) = claims.get_mut(task_id_str).and_then(|claim| claim.arm_mut(arm)) {
            held.measured = Some(measurement);
        }
    });
}

/// The pricing context of an unpriced invocation: its final pair, and the
/// book the run selected checked for that pair's exact entry. `missing_entry`
/// is unknown where the pair is. §FS-rhei-budgets.5.2 §FS-rhei-budgets.8.1
fn unpriced_pricing_context(
    provider: Option<&str>,
    model: Option<&str>,
    book: &PriceBook,
    origin: &PriceBookOrigin,
) -> rhei_core::budget::PricingContext {
    let missing_entry = match (provider, model) {
        (Some(_), Some(_)) => Some(price_entry(book, provider, model).is_none()),
        _ => None,
    };
    rhei_core::budget::PricingContext {
        price_book_source: Some(origin.source.to_string()),
        price_book_id: Some(book.price_book_id.clone()),
        price_book_path: origin.path.clone(),
        provider: provider.map(str::to_owned),
        model: model.map(str::to_owned),
        missing_entry,
    }
}

/// End one arm of the visit this claim paid for, at that arm's own exit.
///
/// The arm settles from its own record alone: once a start exists it owes the
/// day what that record said, or the worst case reserved for it when it wrote
/// none; with no start, its whole reservation goes back, which is this engine
/// knowing it never created the subprocess. A sibling still running is not
/// touched. §FS-rhei-budgets.6.2 §FS-rhei-budgets.6.2.1
///
/// The claim's shared part waits for its last live arm, so the edge that arm
/// applies converts the one travel unit the visit holds. A travel unit still
/// held then is one no edge was applied against — a poll wait, a completion
/// that selected nothing, a stall — and it goes back. §FS-rhei-budgets.4.1
fn budget_settle_visit(
    workspace_root: &Path,
    task_id_str: &str,
    arm: Option<&str>,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let Some(arm) = arm else { return };
    let Some(ended) = with_claims(|claims| end_held_arm(claims, task_id_str, arm)) else {
        return;
    };
    // Best effort by design: a visit that ends while the account cannot be
    // opened leaves the unit outstanding, which is the conservative direction.
    // Failing the run over a unit nobody spent would be the other one.
    let Ok(audit) = budget_audit("end a visit") else { return };
    let project_root = budget_project_root(workspace_root);
    let Ok(Some(account)) = Account::locate(&project_root) else { return };
    let Ok(mut journal) = account.open(true) else { return };
    let held = &ended.arm;
    if held.started {
        // The account's own: an arm with no currency — nothing to settle, or a
        // record whose pricing named none — is charged in the one its reserve
        // was taken in rather than in a guess. §FS-rhei-budgets.5.5
        let snapshot_currency = journal.snapshot().ok().and_then(|snapshot| snapshot.currency);
        let fallback = snapshot_currency.as_deref().unwrap_or("USD");
        // Money first, because the travel release below may return early and
        // the amount is owed either way. A dropped settle leaves the reserve
        // outstanding and is surfaced as `unsettled`. §FS-rhei-budgets.6.2
        let measured = held.measured.as_ref();
        let amount = measured.map_or(built_in::SPEND_RESERVE, |m| m.amount_micro);
        let basis = measured.map_or(SpendBasis::Unmeasurable, |m| m.basis);
        let currency = measured.and_then(|m| m.currency.as_deref()).unwrap_or(fallback);
        let context = measured.and_then(|m| m.pricing_context.as_ref());
        let _ = journal.settle_spend_in_context(arm, amount, currency, basis, context, &audit);
    } else {
        // Nothing started, so the whole reservation goes back — its invocation
        // unit, any travel unit it carries, and the worst case it reserved.
        let _ = journal.release_unstarted(arm, &audit);
    }
    if let Some(travel) = &ended.unapplied_travel {
        let _ = journal.release_travel(travel, &audit);
    }
    // The settle is the visit's last receipt, so this is the account the run
    // ends on when it is the run's last visit. §FS-rhei-run-json.2.1
    emit_budget_account(sink, &journal, &ended.ticket, &ended.bounds);
}

/// What one arm's exit takes out of the claim map, decided under its lock.
struct EndedArm {
    arm: HeldArm,
    /// The travel unit to give back: set only when this was the claim's last
    /// live arm and no edge took the unit. §FS-rhei-budgets.6.2.1
    unapplied_travel: Option<String>,
    ticket: String,
    bounds: CountBounds,
}

/// Take `arm` out of its claim, and the claim out of the map once no arm of it
/// is live. §FS-rhei-budgets.6.2.1
fn end_held_arm(
    claims: &mut BTreeMap<String, HeldClaim>,
    task_id_str: &str,
    arm: &str,
) -> Option<EndedArm> {
    let claim = claims.get_mut(task_id_str)?;
    let index = claim.arms.iter().position(|held| held.reservation == arm)?;
    let held = claim.arms.remove(index);
    // An unstarted arm's release returns the travel unit it carries, so the
    // claim no longer holds one for a sibling's edge to convert.
    if !held.started && claim.travel.as_deref() == Some(arm) {
        claim.travel = None;
    }
    let (ticket, bounds) = (claim.ticket.clone(), claim.bounds.clone());
    let unapplied_travel = if claim.arms.is_empty() {
        claims.remove(task_id_str).and_then(|claim| claim.travel)
    } else {
        None
    };
    Some(EndedArm { arm: held, unapplied_travel, ticket, bounds })
}
