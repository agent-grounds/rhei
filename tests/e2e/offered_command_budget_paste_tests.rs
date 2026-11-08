//! The budget account's offered remedies — set the witness aside, cut it back
//! to its last whole receipt, restore the journal — stay whole on the line that
//! offers them, on every surface that offers them: `rhei budget show`, `rhei
//! budget forget`, and the refusal a charge raises. They print absolute paths
//! under the state directory, so at 80 columns each one was split, and the
//! pasted line did nothing it promised. §FS-rhei-errors.2.1
//!
//! Each case reaches its sub-case the way the budget suites do, takes the line
//! holding the lead-in, pastes only what follows it, and requires the account
//! to reach the state the sentence promised: the next charge goes through.

// §FS-rhei-errors.2.1 §FS-rhei-budgets.5.4.2 §FS-rhei-budgets.5.4.3 §FS-rhei-budgets.10

use std::fs;
use std::io::Write as _;
use std::process::{Command, Output};

use super::budget_torn_witness_support::*;
use super::offered_command_paste_support::*;
use super::*;

const SET_ASIDE: &str = "set the witness aside, keeping every byte, with: ";
const CORE_SET_ASIDE: &str = "set the witness aside instead, keeping every byte: ";
const TRIM: &str = "keep the witness and cut it back to its last whole receipt with: ";
const CORE_TRIM: &str = "cut it back to its last whole receipt with: ";
const RESTORE: &str = "restore this project's journal with: ";

/// The surface a remedy is read from.
#[derive(Clone, Copy)]
enum Surface {
    Show,
    Forget,
    Charge,
}

/// Run `surface` against the world from its project directory, rendered at the
/// captured 80-column default, and require it to refuse.
fn refused_on(world: &World, surface: Surface) -> String {
    let plan = world.plan().display().to_string();
    let mut cmd: Command = match surface {
        Surface::Show => {
            let mut cmd = world.rhei();
            cmd.args(["budget", "show", &plan]);
            cmd
        }
        Surface::Forget => {
            let mut cmd = world.rhei();
            cmd.args(["budget", "forget", &plan, "--reason", "asked anyway"]);
            cmd
        }
        Surface::Charge => world.edge_command("work", "review"),
    };
    let out: Output =
        cmd.current_dir(&world.project).env("NO_COLOR", "1").output().expect("rhei should run");
    let rendered = raw_stderr(&out);
    assert!(!out.status.success(), "the damaged account is refused:\n{rendered}");
    rendered
}

/// The witness carries a receipt written from another project root past this
/// project's journal: `foreign_tail`. §FS-rhei-budgets.5.4.2
fn contaminated(prefix: &str) -> (World, Vec<u8>) {
    let world = world(prefix);
    let elsewhere = world.project.with_file_name("elsewhere");
    let receipt = serde_json::json!({
        "kind": "identity",
        "payload": {"source_path": elsewhere.join("plan.rhei.md")},
    });
    let mut witness =
        fs::OpenOptions::new().append(true).open(world.witness()).expect("open the witness");
    writeln!(witness, "{receipt}").expect("append a foreign receipt");
    drop(witness);
    let bytes = read(&world.witness());
    (world, bytes)
}

/// Paste the set-aside and require what it promised: every byte kept under a
/// set-aside name, and the next charge going through.
#[cfg(unix)]
fn assert_set_aside_pasted(world: &World, pasted: &str, contaminated: &[u8]) {
    let witness_dir = world.witness().parent().expect("the witness directory").to_path_buf();
    paste_ok(&world.project, pasted);
    let kept: Vec<_> = fs::read_dir(&witness_dir)
        .expect("the witness directory")
        .map(|entry| entry.expect("witness entry").path())
        .filter(|path| path.to_string_lossy().contains("history-set-aside-"))
        .collect();
    assert_eq!(kept.len(), 1, "the set-aside kept the witness beside it: {kept:?}");
    assert!(read(&kept[0]) == contaminated, "every byte of the witness is kept");
    assert_success(&world.edge("work", "review"));
}

fn set_aside_offered_whole_on(prefix: &str, surface: Surface, lead_in: &str) {
    let (world, bytes) = contaminated(prefix);
    let rendered = refused_on(&world, surface);

    let pasted = offered_whole(&rendered, lead_in, None, 80);

    #[cfg(unix)]
    assert_set_aside_pasted(&world, &pasted, &bytes);
    #[cfg(not(unix))]
    let _ = (pasted, bytes);
}

/// `rhei budget show` offers the set-aside whole. §FS-rhei-budgets.5.4.2
#[test]
#[ignore = "red until #583: budget show's set-aside is split across lines at 80 columns; implement removes this"]
fn budget_show_offers_the_set_aside_whole() {
    set_aside_offered_whole_on("offered-set-aside-show", Surface::Show, SET_ASIDE);
}

/// `rhei budget forget` refuses a foreign tail offering the set-aside whole.
/// §FS-rhei-budgets.5.4.2 §FS-rhei-budgets.10
#[test]
#[ignore = "red until #583: budget forget's set-aside is split across lines at 80 columns; implement removes this"]
fn budget_forget_offers_the_set_aside_whole() {
    set_aside_offered_whole_on("offered-set-aside-forget", Surface::Forget, SET_ASIDE);
}

/// A charge refused for a foreign tail offers the set-aside whole, in the
/// core's own words. Green before #583, because the same diagnostic's help
/// names `rhei budget show <target>` and §FS-rhei-errors.1.2.1 already keeps it
/// unwrapped; it guards that the core lead-in stays whole if that help changes.
/// §FS-rhei-budgets.5.4.2
#[test]
fn a_refused_charge_offers_the_set_aside_whole() {
    set_aside_offered_whole_on("offered-set-aside-charge", Surface::Charge, CORE_SET_ASIDE);
}

fn trim_offered_whole_on(prefix: &str, surface: Surface, lead_in: &str) {
    let world = world(prefix);
    let whole = world.tear();
    let torn = read(&world.witness());
    let trim = world.damaged()["trim"].as_str().expect("the trim is offered").to_owned();
    let rendered = refused_on(&world, surface);

    let pasted = offered_whole(&rendered, lead_in, None, 80);

    assert_eq!(pasted, trim, "the offered line is the trim the report names");
    #[cfg(unix)]
    {
        paste_ok(&world.project, &pasted);
        let kept = world.kept();
        assert_eq!(kept.len(), 1, "the trim kept one copy beside the witness: {kept:?}");
        assert!(read(&kept[0]) == torn, "the kept copy holds every byte of the torn witness");
        assert!(read(&world.witness()) == whole, "the witness is cut back to a whole receipt");
        assert_success(&world.edge("work", "review"));
    }
    #[cfg(not(unix))]
    let _ = (whole, torn);
}

/// `rhei budget show` offers the trim whole. §FS-rhei-budgets.5.4.3
#[test]
#[ignore = "red until #583: budget show's trim is split across lines at 80 columns; implement removes this"]
fn budget_show_offers_the_trim_whole() {
    trim_offered_whole_on("offered-trim-show", Surface::Show, TRIM);
}

/// `rhei budget forget` refuses a torn witness offering the trim whole.
/// §FS-rhei-budgets.5.4.3 §FS-rhei-budgets.10
#[test]
#[ignore = "red until #583: budget forget's trim is split across lines at 80 columns; implement removes this"]
fn budget_forget_offers_the_trim_whole() {
    trim_offered_whole_on("offered-trim-forget", Surface::Forget, TRIM);
}

/// A charge refused for a torn witness offers the trim whole, in the core's
/// own words. Green before #583 for the reason the set-aside case gives.
/// §FS-rhei-budgets.5.4.3
#[test]
fn a_refused_charge_offers_the_trim_whole() {
    trim_offered_whole_on("offered-trim-charge", Surface::Charge, CORE_TRIM);
}

fn restore_offered_whole_on(prefix: &str, surface: Surface) {
    let world = world(prefix);
    let journal = world.journal();
    let text = fs::read_to_string(&journal).expect("read the journal");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let last = lines.pop().expect("the journal holds a receipt");
    let mut receipt: serde_json::Value = serde_json::from_str(&last).expect("a JSON receipt");
    receipt["previous_hash"] = serde_json::json!("0".repeat(64));
    lines.push(receipt.to_string());
    fs::write(&journal, format!("{}\n", lines.join("\n"))).expect("break the journal's chain");
    let restore = world.damaged()["restore"].as_str().expect("the restore is offered").to_owned();
    let rendered = refused_on(&world, surface);

    let pasted = offered_whole(&rendered, RESTORE, None, 80);

    assert_eq!(pasted, restore, "the offered line is the restore the report names");
    #[cfg(unix)]
    {
        paste_ok(&world.project, &pasted);
        assert_success(&world.edge("work", "review"));
    }
}

/// `rhei budget show` offers the restore whole. §FS-rhei-budgets.5.4
#[test]
#[ignore = "red until #583: budget show's restore is split across lines at 80 columns; implement removes this"]
fn budget_show_offers_the_restore_whole() {
    restore_offered_whole_on("offered-restore-show", Surface::Show);
}

/// `rhei budget forget` refuses this project's own journal offering the
/// restore whole. §FS-rhei-budgets.5.4 §FS-rhei-budgets.10
#[test]
#[ignore = "red until #583: budget forget's restore is split across lines at 80 columns; implement removes this"]
fn budget_forget_offers_the_restore_whole() {
    restore_offered_whole_on("offered-restore-forget", Surface::Forget);
}
