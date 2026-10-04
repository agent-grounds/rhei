// The plan-writer skill is an executable part of authoring: when the CLI is
// unavailable, its no-CLI guidance must select the same state-machine source as
// the resolver. Exercise both source routes promised by
// install-skills instead of reading the checkout file directly.

// §FS-rhei-plan-language.1.3 §FS-rhei-install-skills.4.3

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const SECTION_HEADING: &str = "#### No-CLI state-machine resolution";

/// The three clauses, in the order resolution tries them.
const CLAUSES: [&str; 3] =
    ["**own execution root**", "**project root**", "built-in `rhei` machine"];

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

fn no_cli_section(skill: &str) -> &str {
    let allowed = allowed_states_section(skill);
    allowed
        .split_once(&format!("{SECTION_HEADING}\n"))
        .unwrap_or_else(|| {
            panic!(
                "installed plan-writer is missing `{SECTION_HEADING}`; actual installed Allowed \
                 States guidance:\n{allowed}"
            )
        })
        .1
        .split("\n#### ")
        .next()
        .expect("the no-CLI subsection should exist")
}

fn assert_resolution_contract(skill: &str) {
    let section = no_cli_section(skill);
    let mut previous = 0;
    for clause in CLAUSES {
        let at = section
            .find(clause)
            .unwrap_or_else(|| panic!("no-CLI guidance is missing {clause:?}:\n{section}"));
        assert!(at >= previous, "no-CLI guidance lists {clause:?} out of order:\n{section}");
        previous = at;
    }
    assert!(
        section.contains("another rhei's root never supplies a machine"),
        "no-CLI guidance must rule out the cross-root match:\n{section}"
    );
    assert!(
        !section.contains("**States:**"),
        "no-CLI guidance must not teach the retired line:\n{section}"
    );
}

fn binary_outside_checkout(dir: &Path) -> PathBuf {
    let destination = dir.join("rhei");
    // Copy in a separate process so parallel test forks cannot inherit the
    // destination's write descriptor and keep this executable ETXTBSY.
    let copy = Command::new(python_command())
        .args(["-c", "import shutil, sys; shutil.copy2(sys.argv[1], sys.argv[2])"])
        .arg(rhei_binary())
        .arg(&destination)
        .output()
        .expect("copy the rhei binary outside checkout discovery");
    let copy = CliRun::from(&copy);
    assert!(copy.status.success(), "copy failed: {}", copy.stderr);
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
