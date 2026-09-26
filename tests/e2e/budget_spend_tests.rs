//! A project's measured spend is bounded by the day, and the bound is in force
//! without anyone declaring anything.
//!
//! The two counts of the families beside this one stop a project that is
//! *spinning*. Neither stops one that is working expensively: a review session
//! on a large diff stays inside its timeout and inside both counts and still
//! costs real money, and ten of them cost ten times as much with nothing in the
//! way. What this family pins is the third dimension — charged from the cost
//! accounting record an invocation already produces, reserved before each
//! request at a worst case, and settled afterwards to what was actually spent.
//!
//! Two properties are load-bearing in almost every case below and are easy to
//! lose. An invocation Rhei cannot price, or cannot measure at all, is charged
//! the worst case and **never nothing** — otherwise the bound is silently
//! absent for exactly the transports no other bound watches. And the settle
//! **revises**: the day carries what the request cost, not what was reserved
//! for it, which is the one place money is allowed to behave unlike a count.

// §FS-rhei-budgets.1 §FS-rhei-budgets.3.4 §FS-rhei-budgets.6.2 §FS-rhei-budgets.8
// §REQ-bounded-neural-work.3

use super::budget_support::*;
use super::*;

/// The built-in price book's one entry, so a record prices rather than reading
/// `unpriced`. §FS-rhei-cost-accounting.5.1
const PRICED_MODEL: &str = "claude-sonnet-4-6";
const PRICED_TARGET: &str = "claude-code[yolo]:anthropic:claude-sonnet-4-6";

/// A model the built-in price book has no entry for, so its record is written
/// and measured but never priced. §FS-rhei-cost-accounting.5
const UNPRICED_MODEL: &str = "claude-opus-9";
const UNPRICED_TARGET: &str = "claude-code[yolo]:anthropic:claude-opus-9";

/// An agent id no accounting extractor is chosen by, so the invocation leaves
/// **no record at all** — the case a settle-side fallback could never reach.
/// §FS-rhei-cost-accounting.3.2
const UNMEASURABLE_TARGET: &str = "mock[yolo]:anthropic:claude-sonnet-4-6";

/// The ceiling reached before the third request: two invocations at $3.00 each
/// leave $6.00 on the day, and $6.00 plus the next $20.00 reserve is past $25.00.
const CEILING: &str = r#", "spend_per_day": 25.00"#;

/// The sharpest case in the family, and the one the ticket is written around.
///
/// Two requests run, their real cost is on the ledger, and the third is refused
/// *before* it starts rather than discovered afterwards. The ticket stays where
/// it stood with its artifacts intact, which is the existing stall contract
/// rather than a new one, and the halt says all six things an operator needs in
/// the order every other halt says them in.
///
/// The arithmetic is deliberately legible: `$6.00 + $20.00 > $25.00`. A halt
/// whose numbers do not add up in the reader's head has not said what stopped
/// the work, which is why `outstanding:` carries the refused request's own
/// worst case rather than only what the ledger already held.
// §FS-rhei-budgets.8 §FS-rhei-budgets.6.2
#[test]
fn a_project_at_the_spend_ceiling_halts_before_the_request_that_would_exceed_it() {
    let (dir, plan, machine) = setup_spend(
        "budget-spend-halt",
        &spend_ping_pong(PRICED_TARGET),
        PRICED_AGENT,
        "claude-code",
        PRICED_MODEL,
        CEILING,
    );
    let before = fs::read_to_string(&plan).expect("read the plan");

    let result = run_plan(&plan, &machine, Some(SPEND_DAY));

    assert_spawn_count(&dir, 2, "two requests fit under $25.00; the third's worst case does not");
    assert!(!result.status.success(), "a run halted on spend exits non-zero");
    assert_halt_mentions(&result, "has spent today's measured budget");
    assert_halt_mentions(&result, "dimension:   project spend");
    assert_halt_mentions(&result, "bound:       $25.00 (machine)");
    assert_halt_mentions(&result, "consumed:    $6.00  outstanding: $20.00  remaining: $0.00");
    assert_halt_mentions(&result, &format!("mode:        {SPEND_DAY_KEY}"));
    assert_halt_mentions(&result, &format!("renews at:   {SPEND_RENEWAL}"));
    assert!(
        !format!("{}{}", result.stdout, result.stderr).contains("to raise it:"),
        "a window limiter's one remedy is the renewal instant, not a settings key; got:\n{}\n{}",
        result.stdout,
        result.stderr
    );
    assert_plan_but_for_the_budget_identity(
        &fs::read_to_string(&plan).expect("read the plan"),
        &before,
        "a refused admission leaves the ticket exactly where it stood",
    );
}

/// The day carries what was spent, not what was reserved for it.
///
/// This is the whole of "the settle revises". A reserve that stayed on the day
/// would make a $400.00 ceiling stop after twenty agents instead of the
/// hundred-odd the measured median implies, and would report a number nobody
/// spent — so the ledger would be both wrong and needlessly tight.
// §FS-rhei-budgets.6.2 §REQ-bounded-neural-work.4
#[test]
fn a_reserve_settles_down_to_the_measured_amount_and_the_day_carries_that() {
    let (_dir, plan, machine) = setup_spend(
        "budget-spend-settle",
        &spend_finishing(PRICED_TARGET),
        PRICED_FINISHING_AGENT,
        "claude-code",
        PRICED_MODEL,
        "",
    );

    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    let shown = budget_show(&plan, Some(SPEND_DAY));

    assert_success(&shown);
    assert!(
        shown.stdout.contains(&format!(
            "project spend: {PRICED_INVOCATION} consumed + $0.00 outstanding / $400.00"
        )),
        "the day carries the measured $3.00 and holds nothing outstanding, \
         not the {SPEND_RESERVE} that was reserved for it; got:\n{}",
        shown.stdout
    );
    assert!(
        shown.stdout.contains("$397.00 remaining"),
        "the remainder is the ceiling less what was actually spent; got:\n{}",
        shown.stdout
    );
}

/// Nobody who wrote nothing is refused, and nobody who wrote nothing is left
/// guessing what they are bounded by. `spend_per_day` is not declarable on a
/// plan at all, so the built-in is what every plan runs under until a machine
/// or a project says otherwise.
// §REQ-bounded-neural-work.2 §FS-rhei-budgets.2.1 §FS-rhei-validate.4
#[test]
fn a_plan_that_declares_nothing_runs_under_the_built_in_spend_ceiling() {
    let (dir, plan, machine) = setup_spend(
        "budget-spend-declaration-free",
        &spend_finishing(PRICED_TARGET),
        PRICED_FINISHING_AGENT,
        "claude-code",
        PRICED_MODEL,
        "",
    );

    let validated = run_at("validate", &plan, &machine, None, &[]);
    assert_success(&validated);
    assert!(
        validated.stdout.contains("spend_per_day: 400.00 (built_in)"),
        "the fourth bound is reported with its built-in value and source, as \
         the bare number its settings key takes; got:\n{}",
        validated.stdout
    );

    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    assert_task_state(&plan, &machine, "1", "completed");
    assert_eq!(spawns(&dir), ["work"], "the built-in ceiling is far above one request");
}

/// A record that was measured but could not be priced is charged the worst
/// case, and the surfaces say how much of the day that was.
///
/// Charging it `$0.00` would be the quiet failure: the ceiling would read
/// healthy while the money went out, and it would read healthy hardest for the
/// models nobody has written a price for yet.
// §FS-rhei-budgets.6.2
#[test]
fn an_invocation_the_price_book_cannot_price_is_charged_the_reserve() {
    let (_dir, plan, machine) = setup_spend(
        "budget-spend-unpriced",
        &spend_finishing(UNPRICED_TARGET),
        PRICED_FINISHING_AGENT,
        "claude-code",
        UNPRICED_MODEL,
        "",
    );

    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    let shown = budget_show(&plan, Some(SPEND_DAY));

    assert_success(&shown);
    assert!(
        shown.stdout.contains(&format!("project spend: {SPEND_RESERVE} consumed")),
        "an unpriced invocation is charged the {SPEND_RESERVE} worst case, never \
         nothing; got:\n{}",
        shown.stdout
    );
    assert!(
        shown
            .stdout
            .contains(&format!("estimated:   1 unpriced (charged at {SPEND_RESERVE} each)")),
        "the surfaces distinguish an estimated day from a measured one; got:\n{}",
        shown.stdout
    );
}

/// The case no settle can reach, and therefore the one that decides where the
/// fallback has to be charged.
///
/// Usage extraction is chosen by the agent's id, so an agent that has none
/// writes no accounting record at all — not an unpriced one, not an empty one,
/// none. A fallback applied when a record arrives would add nothing here and
/// the day would read `$0.00` forever. Charging at the **reserve**, which runs
/// because admission always runs, is what makes the class visible.
// §FS-rhei-budgets.6.2 §FS-rhei-cost-accounting.3.2
#[test]
fn an_invocation_with_no_accounting_record_is_charged_at_the_reserve() {
    let (dir, plan, machine) = setup_spend(
        "budget-spend-unmeasurable",
        &spend_finishing(UNMEASURABLE_TARGET),
        PRICED_FINISHING_AGENT,
        "mock",
        PRICED_MODEL,
        "",
    );

    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    assert!(
        !dir.join("runtime/accounting/invocations").exists()
            || fs::read_dir(dir.join("runtime/accounting/invocations"))
                .expect("read invocations")
                .next()
                .is_none(),
        "the premise of this case is that no accounting record was written at all"
    );

    let shown = budget_show(&plan, Some(SPEND_DAY));
    assert_success(&shown);
    assert!(
        shown.stdout.contains(&format!("project spend: {SPEND_RESERVE} consumed")),
        "an unmeasurable invocation is charged the worst case rather than \
         being free; got:\n{}",
        shown.stdout
    );
    assert!(
        shown
            .stdout
            .contains(&format!("estimated:   1 unmeasurable (charged at {SPEND_RESERVE} each)")),
        "`unmeasurable` is reported apart from `unpriced`: one has a record \
         nobody could price, the other has no record; got:\n{}",
        shown.stdout
    );
}

/// The window is a key rather than a refill, for money exactly as for the
/// count. Nothing is granted at midnight; the new day simply has its own key,
/// and yesterday's amounts stay stamped with yesterday.
// §FS-rhei-budgets.3.4 §FS-rhei-budgets.3.3
#[test]
fn the_next_utc_day_starts_from_nothing_consumed() {
    let (_dir, plan, machine) = setup_spend(
        "budget-spend-window",
        &spend_finishing(PRICED_TARGET),
        PRICED_FINISHING_AGENT,
        "claude-code",
        PRICED_MODEL,
        "",
    );

    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    let first_day = budget_show(&plan, Some(SPEND_DAY));
    assert!(
        first_day.stdout.contains(&format!("project spend: {PRICED_INVOCATION} consumed")),
        "the premise of this case is that the first day carries the spend; got:\n{}",
        first_day.stdout
    );

    let next_day = budget_show(&plan, Some("2026-09-27T09:00:00Z"));

    assert_success(&next_day);
    assert!(
        next_day.stdout.contains("project spend: $0.00 consumed + $0.00 outstanding"),
        "the next UTC day has its own key and carries none of yesterday's \
         spend; got:\n{}",
        next_day.stdout
    );
    assert!(
        next_day.stdout.contains("window (2026-09-27Z)"),
        "the snapshot names the day it is about; got:\n{}",
        next_day.stdout
    );
}

/// A project may lower the machine's ceiling and is honored; one that asks
/// above it is accepted, clamped, and reported as clamped. Refusing it would
/// make a settings file written on one machine invalid on the next; honoring it
/// would let a project raise the cap of whichever machine pays for it.
// §FS-rhei-budgets.2 §FS-rhei-budgets.2.3 §FS-rhei-validate.4
#[test]
fn a_project_asking_above_the_machines_spend_ceiling_is_clamped_and_says_so() {
    let (dir, plan, machine) = setup_spend(
        "budget-spend-clamped",
        &spend_finishing(PRICED_TARGET),
        PRICED_FINISHING_AGENT,
        "claude-code",
        PRICED_MODEL,
        CEILING,
    );
    let settings = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("project settings directory");
    fs::write(settings.join("settings.json"), r#"{ "defaults": { "spend_per_day": 100.00 } }"#)
        .expect("write project settings");

    let result = run_at("validate", &plan, &machine, None, &[]);

    assert_success(&result);
    assert!(
        result.stdout.contains(
            "spend_per_day: 25.00 (requested 100.00 by the project, limited by machine settings)"
        ),
        "the clamped bound names the requester and the limiter in the one line \
         every dimension reports it in; got:\n{}",
        result.stdout
    );
}

/// One account, one currency, fixed by the first amount it ever carried.
///
/// The accounting root has its own per-root currency check, so this case makes
/// sure it is the *account* that refuses: `rhei reset` deletes `runtime/` and
/// with it every invocation record, while the account lives outside `runtime/`
/// and survives. What is left is a project whose accounting root is empty and
/// whose ledger still holds USD — and a second currency there must still be
/// refused before any agent starts, because converting it would need an
/// exchange rate this ledger does not hold and silently adding it would make
/// every number above it wrong.
// §FS-rhei-budgets.5.5 §FS-rhei-budgets.5.1
#[test]
fn a_second_currency_in_one_project_is_refused_before_any_agent_starts() {
    let (dir, plan, machine) = setup_spend(
        "budget-spend-currency",
        &spend_finishing(PRICED_TARGET),
        PRICED_FINISHING_AGENT,
        "claude-code",
        PRICED_MODEL,
        "",
    );
    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    assert_success(&run_at("reset", &plan, &machine, None, &["--yes"]));

    let prices = write_fixture_file(
        &dir,
        "chf-prices.json",
        r#"{
  "schema": "rhei.accounting.prices.v1",
  "price_book_id": "fixture-chf-2026-09-01",
  "currency": "CHF",
  "entries": [{
    "provider": "anthropic",
    "model": "claude-sonnet-4-6",
    "effective_at": "2026-09-01T00:00:00Z",
    "unit": "1m_tokens",
    "input_total_micro": 3000000,
    "input_cached_read_micro": 300000,
    "input_cache_write_micro": 3750000,
    "output_total_micro": 15000000
  }]
}"#,
    );
    let prices_arg = prices.to_string_lossy().into_owned();
    let before = spawns(&dir).len();

    let result = run_at(
        "run",
        &plan,
        &machine,
        Some(SPEND_DAY),
        &["--no-tui", "--no-callbacks", "--prices", &prices_arg],
    );

    assert!(!result.status.success(), "a second currency in one account is refused");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("CHF") && combined.contains("USD"),
        "the refusal names both currencies, so the reader knows which is the \
         account's and which arrived; got:\n{combined}"
    );
    assert_eq!(
        spawns(&dir).len(),
        before,
        "the refusal comes before any agent starts; spawns={:?}",
        spawns(&dir)
    );
}

/// A price book denominated in something other than the dollar this binary was
/// written in, so the account it establishes is too. §FS-rhei-cost-accounting.5
const EUR_PRICES: &str = r#"{
  "schema": "rhei.accounting.prices.v1",
  "price_book_id": "fixture-eur-2026-09-01",
  "currency": "EUR",
  "entries": [{
    "provider": "anthropic",
    "model": "claude-sonnet-4-6",
    "effective_at": "2026-09-01T00:00:00Z",
    "unit": "1m_tokens",
    "input_total_micro": 3000000,
    "input_cached_read_micro": 300000,
    "input_cache_write_micro": 3750000,
    "output_total_micro": 15000000
  }]
}"#;

/// A `claude-code` spawn that reports no usage at all.
///
/// The extractor is chosen by the agent id, so a record *is* written — and with
/// no token measured in it, its pricing status is `not-applicable`: nothing
/// measured, nothing priced, and **no currency named**, which is the arm the
/// case below is about. §FS-rhei-cost-accounting.5
const SILENT_FINISHING_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'spawn-count.log', '{}\n'.format(env('RHEI_STATE')))
result('done\n')
"#;

/// An amount with no currency of its own is settled in the account's, not in a
/// guess.
///
/// One account holds exactly one currency (§FS-rhei-budgets.5.5), so a settle
/// that stamped `USD` on a record that named none would be a foreign receipt to
/// a ledger denominated in anything else: refused, and refused *silently*,
/// because a visit that ends may not fail a run over a unit nobody spent. What
/// would be left is the reserve standing as `unsettled` — and the mark is the
/// whole point of keeping three of them. `unsettled` sends an operator after a
/// run that went away; this day wants a price for the model it ran.
// §FS-rhei-budgets.5.5 §FS-rhei-budgets.6.2
#[test]
fn a_record_that_names_no_currency_is_settled_in_the_accounts_own() {
    let (dir, plan, machine) = setup_spend(
        "budget-spend-eur",
        &spend_finishing(PRICED_TARGET),
        SILENT_FINISHING_AGENT,
        "claude-code",
        PRICED_MODEL,
        "",
    );
    let prices = write_fixture_file(&dir, "eur-prices.json", EUR_PRICES);
    let prices_arg = prices.to_string_lossy().into_owned();

    assert_success(&run_at(
        "run",
        &plan,
        &machine,
        Some(SPEND_DAY),
        &["--no-tui", "--no-callbacks", "--prices", &prices_arg],
    ));
    let shown = budget_show(&plan, Some(SPEND_DAY));

    assert_success(&shown);
    assert!(
        shown.stdout.contains("project spend: 20.00 EUR consumed + 0.00 EUR outstanding"),
        "the worst case is charged to the day in the account's own currency \
         rather than left outstanding by a receipt the ledger refused; got:\n{}",
        shown.stdout
    );
    assert!(
        shown.stdout.contains("estimated:   1 unpriced (charged at 20.00 EUR each)"),
        "and it carries the mark that says what to do about it, not the one \
         that says a run disappeared; got:\n{}",
        shown.stdout
    );
}
