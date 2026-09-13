// The plan-writer skill is an executable part of authoring: when the CLI is
// unavailable, its bounded decision table must select the same state-machine
// source as the resolver. Exercise both source routes promised by
// install-skills instead of reading the checkout file directly.

// §FS-rhei-plan-language.1.3 §FS-rhei-install-skills.4.3

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const TABLE_HEADING: &str = "#### No-CLI state-machine resolution";
const TABLE_COLUMNS: [&str; 4] =
    ["Invocation scope", "Effective `**States:**`", "Matching file available", "Resolution"];

const EXPECTED_ROWS: [[&str; 4]; 5] = [
    [
        "Panta project default (inherited or restated)",
        "`rhei`",
        "project-root `states.yaml`",
        "project-root file",
    ],
    [
        "Panta project default (inherited or restated)",
        "`rhei`",
        "member-root `states.yaml` only",
        "built-in `rhei`",
    ],
    ["Standalone single-file plan", "`rhei`", "sibling `states.yaml`", "sibling file"],
    [
        "Standalone Directory Workspace",
        "`rhei`",
        "workspace-root `states.yaml`",
        "workspace-root file",
    ],
    [
        "Panta custom project default",
        "non-`rhei`",
        "one matching member-root `states.yaml`",
        "unique member-root file",
    ],
];

fn install_plan_writer(bin: &Path, cwd: &Path, prefix: &str) -> String {
    let home = unique_temp_dir(prefix);
    let mut cmd = rhei_process_at(bin);
    cmd.env("HOME", &*home).env("XDG_STATE_HOME", home.join("state")).current_dir(cwd).args([
        "install-skills",
        "--agent",
        "claude-code",
        "--skills",
        "rhei-plan-writer",
    ]);
    let result = CliRun::from(&cmd.output().expect("rhei install-skills should run"));
    assert_success(&result);

    fs::read_to_string(home.join(".claude/skills/rhei-plan-writer/SKILL.md"))
        .expect("read the installed plan-writer skill")
}

fn allowed_states_section(skill: &str) -> &str {
    skill
        .split_once("### Allowed States\n")
        .expect("installed plan-writer should have an Allowed States section")
        .1
        .split_once("\n### ID Policy")
        .expect("Allowed States should end at ID Policy")
        .0
}

fn resolution_rows(skill: &str) -> Vec<Vec<&str>> {
    let allowed = allowed_states_section(skill);
    let table = allowed
        .split_once(&format!("{TABLE_HEADING}\n"))
        .unwrap_or_else(|| {
            panic!(
                "installed plan-writer is missing `{TABLE_HEADING}` and its bounded decision \
                 table; expected the Panta `rhei` default with only a member-root match to \
                 resolve to built-in `rhei`. Actual installed Allowed States guidance:\n{allowed}"
            )
        })
        .1
        .split("\n#### ")
        .next()
        .expect("the no-CLI subsection should exist");

    let mut lines = table.lines().filter(|line| line.trim().starts_with('|'));
    let columns = table_cells(lines.next().expect("the decision table should have a header"));
    assert_eq!(columns, TABLE_COLUMNS, "unexpected no-CLI decision-table columns");
    let separator = table_cells(lines.next().expect("the decision table should have a separator"));
    assert!(
        separator.iter().all(|cell| cell.chars().all(|ch| ch == '-' || ch == ':')),
        "invalid no-CLI decision-table separator: {separator:?}"
    );
    lines.map(table_cells).collect()
}

fn table_cells(line: &str) -> Vec<&str> {
    line.trim().trim_matches('|').split('|').map(str::trim).collect()
}

fn assert_resolution_contract(skill: &str) {
    let rows = resolution_rows(skill);
    for expected in EXPECTED_ROWS {
        assert!(
            rows.iter().any(|row| row == &expected),
            "installed no-CLI decision table is missing the resolution {expected:?}; rows: {rows:#?}"
        );
    }
}

fn binary_outside_checkout(dir: &Path) -> PathBuf {
    let destination = dir.join("rhei");
    fs::copy(rhei_binary(), &destination).expect("copy the rhei binary outside checkout discovery");
    destination
}

fn assert_no_filesystem_skill_source(path: &Path) {
    for ancestor in path.ancestors() {
        assert!(
            !ancestor.join("crates/rhei-cli/skills").is_dir(),
            "test path unexpectedly discovers checkout skills at {}",
            ancestor.display()
        );
        assert!(
            !ancestor.join("share/rhei/skills").is_dir(),
            "test binary unexpectedly discovers packaged skills at {}",
            ancestor.display()
        );
    }
}

/// Running the ordinary test binary from the repository root establishes the
/// checkout source: source discovery must win, and byte equality with the
/// checkout artifact proves which copy was installed.
/// §FS-rhei-install-skills.4.3
#[test]
fn issue_244_contract_checkout_guidance_matches_state_machine_resolution() {
    let checkout = repo_root();
    let installed =
        install_plan_writer(&rhei_binary(), &checkout, "state-guidance-checkout-install");
    let source =
        fs::read_to_string(checkout.join("crates/rhei-cli/skills/rhei-plan-writer/SKILL.md"))
            .expect("read checkout plan-writer skill");
    assert_eq!(installed, source, "install did not use the checkout skill source");

    assert_resolution_contract(&installed);
}

/// Copying the binary and running it under separate scratch roots establishes
/// the embedded source: neither binary nor cwd has a checkout or packaged asset
/// directory in its ancestry, so filesystem discovery cannot satisfy install.
/// §FS-rhei-install-skills.4.3
#[test]
fn issue_244_contract_embedded_guidance_matches_state_machine_resolution() {
    let bin_dir = unique_temp_dir("state-guidance-embedded-bin");
    let cwd = unique_temp_dir("state-guidance-embedded-cwd");
    let bin = binary_outside_checkout(&bin_dir);
    assert_no_filesystem_skill_source(&bin);
    assert_no_filesystem_skill_source(&cwd);

    let installed = install_plan_writer(&bin, &cwd, "state-guidance-embedded-install");
    assert_resolution_contract(&installed);
}
