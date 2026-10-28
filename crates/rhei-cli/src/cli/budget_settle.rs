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
/// sequential and the parallel path reach it the same way. `None` is the
/// class with no accounting extractor at all — no record is ever written for
/// it — and it is deliberately *not* recorded here, so the settle charges it
/// the reserve as `unmeasurable` rather than as nothing.
/// §FS-rhei-budgets.6.2 §FS-rhei-cost-accounting.3.2
fn budget_record_spend(
    task_id_str: &str,
    usage: Option<&rhei_tui::UsageSummary>,
    book: &PriceBook,
    origin: &PriceBookOrigin,
) {
    let Some(usage) = usage else { return };
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
        if let Some(claim) = claims.get_mut(task_id_str) {
            claim.measured.push(measurement);
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

/// End the visit this claim paid for.
///
/// A travel unit still held is one no edge was applied against — a poll wait, a
/// completion that selected nothing, a stall — and it goes back. An invocation
/// unit goes back only when no start was ever recorded, which is this engine
/// knowing it never created the subprocess; once a start exists the unit is
/// consumed, ambiguous or not. And once a start exists, every arm owes the
/// day an amount: what the record said, or the worst case that was reserved
/// for it. §FS-rhei-budgets.4.1 §FS-rhei-budgets.6.2
fn budget_settle_visit(
    workspace_root: &Path,
    task_id_str: &str,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let Some(claim) = with_claims(|claims| claims.remove(task_id_str)) else { return };
    // Best effort by design: a visit that ends while the account cannot be
    // opened leaves the unit outstanding, which is the conservative direction.
    // Failing the run over a unit nobody spent would be the other one.
    let Ok(audit) = budget_audit("end a visit") else { return };
    let project_root = budget_project_root(workspace_root);
    let Ok(Some(account)) = Account::locate(&project_root) else { return };
    let Ok(mut journal) = account.open(true) else { return };
    if claim.started {
        // The account's own: an arm with no currency — nothing to settle, or a
        // record whose pricing named none — is charged in the one its reserve
        // was taken in rather than in a guess. §FS-rhei-budgets.5.5
        let held = journal.snapshot().ok().and_then(|snapshot| snapshot.currency);
        let fallback = held.as_deref().unwrap_or("USD");
        // Money first, because the travel release below may return early and
        // the amount is owed either way. A dropped settle leaves the reserve
        // outstanding and is surfaced as `unsettled`. §FS-rhei-budgets.6.2
        for (index, arm) in claim.arms.iter().enumerate() {
            let measured = claim.measured.get(index);
            let amount = measured.map_or(built_in::SPEND_RESERVE, |m| {
                m.amount_micro
            });
            let basis = measured.map_or(SpendBasis::Unmeasurable, |m| m.basis);
            let currency = measured.and_then(|m| m.currency.as_deref()).unwrap_or(fallback);
            let context = measured.and_then(|m| m.pricing_context.as_ref());
            let _ =
                journal.settle_spend_in_context(arm, amount, currency, basis, context, &audit);
        }
        if let Some(travel) = &claim.travel {
            let _ = journal.release_travel(travel, &audit);
        }
    } else {
        // Nothing started, so the whole reservation goes back — its invocation
        // unit, its travel unit, and the worst case it reserved against the day.
        for arm in &claim.arms {
            let _ = journal.release_unstarted(arm, &audit);
        }
    }
    // The settle is the visit's last receipt, so this is the account the run
    // ends on when it is the run's last visit. §FS-rhei-run-json.2.1
    emit_budget_account(sink, &journal, &claim.ticket, &claim.bounds);
}
