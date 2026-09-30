//! Black-box coverage for `defaults.prices`, the settings key that supplies the
//! default value of `rhei run --prices`.
//!
//! The harness gives every test its own `HOME` (`rhei_command`), so the machine
//! tier a test writes is the only machine tier it can see and no real book on
//! the developer's machine reaches these runs.
//! §FS-rhei-cost-accounting.5.1 §FS-rhei-agents.1.1.1

use std::fs;
use std::path::{Path, PathBuf};

use super::accounting_prices_support::*;
use super::roster_support::run_roster;
use super::*;

/// The machine tier `rhei_command` pins, and the directory a relative
/// `defaults.prices` is therefore resolved against. §FS-rhei-agents.1.3
fn machine_settings_dir(dir: &Path) -> PathBuf {
    dir.join(".home/.config/rhei")
}

/// Write `defaults.prices` on the machine tier, as an author naming one book
/// for every run on this machine would. §FS-rhei-agents.1.1.1
fn write_machine_prices(dir: &Path, authored: &str) -> PathBuf {
    let settings_dir = machine_settings_dir(dir);
    fs::create_dir_all(&settings_dir).expect("create machine settings directory");
    let path = settings_dir.join("settings.json");
    fs::write(
        &path,
        format!(
            "{{\n  \"defaults\": {{ \"program_timeout\": \"30m\", \"prices\": {} }}\n}}\n",
            serde_json::to_string(authored).expect("encode authored path")
        ),
    )
    .expect("write machine settings");
    path
}

/// Add `defaults.prices` to the project tier `write_measured_codex_settings`
/// already wrote, leaving its agent and model registries alone.
fn add_project_prices(dir: &Path, authored: &str) {
    let path = dir.join(".agent-grounds/rhei/settings.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read project settings"))
            .expect("parse project settings");
    settings["defaults"]["prices"] = serde_json::json!(authored);
    fs::write(&path, serde_json::to_string_pretty(&settings).expect("serialize project settings"))
        .expect("write project settings");
}

/// Compare paths the same way whichever separator the host writes.
fn portable(text: &str) -> String {
    text.replace('\\', "/")
}

fn priced_workspace(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", PRICED_MACHINE);
    (dir, plan, machine)
}

/// The reported behaviour, from the side a reader meets it: a `prices` key
/// beside `agent_timeout` in the same `defaults` block is shown with the tier
/// that declared it, the authored string unresolved, and a project file
/// replaces it.
// §FS-rhei-agents.1.1.1 §FS-rhei-agents.1.1.7 §FS-rhei-cost-accounting.5.1
#[test]
fn roster_shows_a_settings_price_book_with_the_tier_that_declared_it() {
    let (dir, plan, _machine) = priced_workspace("settings-prices-roster");
    write_measured_codex_settings(&dir, None);
    write_machine_prices(&dir, "~/machine-prices.json");

    let global = run_roster(&dir.join(".home"), &dir, Some(&plan), false);
    assert_success(&global);
    assert!(
        global.stdout.contains("program_timeout: \"30m\" [global]"),
        "the block the key sits in was not read:\n{}",
        global.stdout
    );
    assert!(
        global.stdout.contains("prices: \"~/machine-prices.json\" [global]"),
        "machine price book missing from:\n{}",
        global.stdout
    );

    add_project_prices(&dir, "books/project-prices.json");
    let project = run_roster(&dir.join(".home"), &dir, Some(&plan), false);
    assert_success(&project);
    assert!(
        project.stdout.contains("prices: \"books/project-prices.json\" [project]"),
        "project price book missing from:\n{}",
        project.stdout
    );
    assert!(
        !project.stdout.contains("machine-prices.json"),
        "the project file did not replace the machine book:\n{}",
        project.stdout
    );
}

/// A machine that names one book prices a run that passes no flag, records the
/// book's own id, copies it beside the records, and archives nothing.
// §FS-rhei-cost-accounting.5.1 §FS-rhei-run.2
#[test]
fn a_settings_price_book_prices_a_run_that_passes_no_flag() {
    let (dir, plan, machine) = priced_workspace("settings-prices-run");
    write_measured_codex_settings(&dir, None);
    let book = write_price_book(&dir);
    write_machine_prices(&dir, &book.to_string_lossy());

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);

    assert_success(&result);
    assert_selected_pricing(&dir);
    assert_selected_book_copy(&dir, &price_book_json());
    assert!(
        !dir.join("runtime/accounting/price-books").exists(),
        "a settings book is copied, never archived"
    );
}

/// `--prices` keeps precedence: the settings key is its default value and
/// nothing more. This guards the rule rather than pinning new behaviour — the
/// flag already wins today, and it must keep winning.
// §FS-rhei-cost-accounting.5.1
#[test]
fn the_prices_flag_wins_over_a_settings_price_book() {
    let (dir, plan, machine) = priced_workspace("settings-prices-flag-wins");
    write_measured_codex_settings(&dir, None);
    let mut flag_book = price_book_json();
    flag_book["price_book_id"] = serde_json::json!("flag-luna-2026-09-30");
    let flag_path = write_price_book_value(&dir, "flag-prices.json", &flag_book);
    let settings_book = write_price_book_value(&dir, "settings-prices.json", &price_book_json());
    write_machine_prices(&dir, &settings_book.to_string_lossy());
    let flag_arg = flag_path.to_string_lossy().into_owned();

    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--prices", &flag_arg]);

    assert_success(&result);
    assert_selected_pricing_from(&dir, "flag-luna-2026-09-30");
    assert_selected_book_copy(&dir, &flag_book);
}

/// A relative path is resolved against the directory of the settings file that
/// declared it. The run is started from an unrelated working directory, which
/// is what rules out resolution against the cwd.
// §FS-rhei-agents.1.3 §FS-rhei-cost-accounting.5.1
#[test]
fn a_relative_settings_path_resolves_beside_its_own_settings_file() {
    let (dir, plan, machine) = priced_workspace("settings-prices-relative");
    write_measured_codex_settings(&dir, None);
    write_price_book_value(&machine_settings_dir(&dir), "prices.json", &price_book_json());
    write_machine_prices(&dir, "prices.json");
    assert!(
        !std::env::current_dir().expect("cwd").join("prices.json").exists(),
        "the fixture only proves anything from a directory holding no prices.json"
    );

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);

    assert_success(&result);
    assert_selected_pricing(&dir);
    assert_selected_book_copy(&dir, &price_book_json());
}

/// A leading `~` expands to the home the run reads its settings from.
// §FS-rhei-agents.1.3
#[test]
fn a_tilde_settings_path_expands_to_the_runs_home() {
    let (dir, plan, machine) = priced_workspace("settings-prices-tilde");
    write_measured_codex_settings(&dir, None);
    write_price_book_value(&dir.join(".home"), "home-prices.json", &price_book_json());
    write_machine_prices(&dir, "~/home-prices.json");

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);

    assert_success(&result);
    assert_selected_pricing(&dir);
}

/// A misspelled path stops the run and nothing else: the path is resolved when
/// settings merge and opened only when a run prices, so inspection keeps
/// working and shows the path it was given.
// §FS-rhei-cost-accounting.5.1 §FS-rhei-agents.1.3
#[test]
fn a_settings_price_book_that_names_nothing_stops_only_the_run() {
    let (dir, plan, machine) = priced_workspace("settings-prices-missing");
    let spawned = dir.join("spawned.marker");
    write_measured_codex_settings(&dir, Some(&spawned));
    let missing = dir.join("no-such-book.json");
    let authored = missing.to_string_lossy().into_owned();
    let settings = write_machine_prices(&dir, &authored);

    let roster = run_roster(&dir.join(".home"), &dir, Some(&plan), false);
    assert_success(&roster);
    // The roster prints the value as authored, so compare against the JSON text
    // the settings file carries -- on Windows a separator is `\\` in both.
    let as_written = serde_json::to_string(&authored).expect("encode authored path");
    assert!(
        roster.stdout.contains(&as_written),
        "roster hid the path it was given as {as_written}:\n{}",
        roster.stdout
    );
    assert_success(&run_cli("validate", &plan, &machine, &[]));
    assert_success(&run_cli("list", &plan, &machine, &[]));

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);

    assert!(
        !result.status.success(),
        "a book that cannot be read must fail the run\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let stderr = portable(&result.stderr);
    assert!(stderr.contains(&portable(&authored)), "book path missing from:\n{}", result.stderr);
    assert!(
        stderr.contains(&portable(&settings.to_string_lossy())),
        "declaring settings file missing from:\n{}",
        result.stderr
    );
    assert!(!spawned.exists(), "the fake agent started before the book was read");
}
