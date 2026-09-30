//! `--into` a **member rhei of a Panta project**, where the settings a machine
//! names and the machine a rhei may inherit both resolve at the project rather
//! than at the rhei.
//! §FS-rhei-library.2 §FS-rhei-templates.6.2
//!
//! Two things follow from membership and neither is visible from the rhei
//! alone. The hoist's destination is the project, because a `settings.json`
//! written beside a member is read by nothing — and a union that dropped it
//! there would leave a project whose next `rhei validate` fails on the agent
//! the machine it just brought names. And the validation scope is the project,
//! because that is the document the result has to be correct in: validating the
//! member in isolation is the same mistake §FS-rhei-templates.6.2's
//! "Validation scope" names for `--output`.

use super::into_support::*;
use super::*;

/// The hoist lands in the project, nothing is left beside the rhei, the agent
/// is in the roster every project-scoped command reads, and the project still
/// loads — which is the assertion a copy beside the rhei failed.
/// §FS-rhei-library.2 §FS-rhei-templates.6.2
#[test]
fn a_member_union_hoists_the_templates_settings_to_the_project() {
    let (dir, root) = host_workspace("into-project-hoist");
    make_project(&dir);

    let placed = run_into(&["instantiate", "fix", "--into", "release"], &dir);
    assert_success(&placed);

    assert!(
        !root.join(".agent-grounds/rhei/settings.json").exists(),
        "a member's own settings root is read by nothing, so nothing is left there"
    );
    let hoisted = read(&dir.join(".agent-grounds/rhei/settings.json"));
    assert!(
        hoisted.contains("fix-codex"),
        "the template's agent is the project's; got:\n{hoisted}"
    );
    assert!(
        placed.stdout.contains("added to"),
        "the summary names what the hoist added; got:\n{}",
        placed.stdout
    );

    // What the union wrote is what a project-scoped command reads.
    let roster = run_into(&["roster", "."], &dir);
    assert_success(&roster);
    assert!(
        roster.stdout.contains("fix-codex"),
        "the hoisted agent is in the project's roster; got:\n{}",
        roster.stdout
    );
    assert_success(&run_into(&["validate", "."], &dir));
}

/// Values the project already defines win, and the summary names both halves —
/// what was added and what was kept — so a hoist is never a silent redefinition
/// of a configured agent.
/// §FS-rhei-templates.6.2 §FS-rhei-agents.1.1
#[test]
fn the_project_keeps_the_values_it_already_defines_and_says_so() {
    let (dir, _root) = host_workspace("into-project-precedence");
    make_project(&dir);
    write_settings(
        &dir,
        "{\n  \"agents\": {\n    \"fix-codex\": {\n      \"command\": [\"true\"],\n      \"model_flag\": \"--model\",\n      \"stdin_prompt\": true,\n      \"modes\": { \"xhigh\": [\"--already-the-projects\"] }\n    }\n  }\n}\n",
    );

    let placed = run_into(&["instantiate", "fix", "--into", "release"], &dir);
    assert_success(&placed);
    assert!(
        placed.stdout.contains("kept the project's own values"),
        "the summary names what the project kept; got:\n{}",
        placed.stdout
    );
    let settings = read(&dir.join(".agent-grounds/rhei/settings.json"));
    assert!(
        settings.contains("--already-the-projects"),
        "the project's own value survives the hoist; got:\n{settings}"
    );
    assert_success(&run_into(&["validate", "."], &dir));
}

/// A template whose machine names an agent only the **project** configures is
/// placed successfully, because a member is validated through the project. This
/// is the ordinary case in a workspace whose agents are declared once, and
/// validating the member alone refuses it.
/// §FS-rhei-library.2 §FS-rhei-templates.6.2
#[test]
fn a_member_union_is_validated_in_the_projects_terms() {
    let (dir, _root) = host_workspace("into-project-scope");
    make_project(&dir);
    write_settings(&dir, BORROWED_AGENT_SETTINGS);
    write_borrowed_agent_template(&dir);

    let placed = run_into(&["instantiate", "polish", "--into", "release"], &dir);
    assert_success(&placed);
    assert_success(&run_into(&["validate", "."], &dir));
}

/// The other half of the same rule: outside a project the rhei's own root *is*
/// the scope, so the borrowed agent is out of scope and the union is refused
/// with the target byte-identical. The scope follows membership rather than
/// being one guess for both shapes.
/// §FS-rhei-library.2
#[test]
fn outside_a_project_the_same_union_is_validated_in_the_rheis_own_terms() {
    let (dir, root) = host_workspace("into-rhei-scope");
    write_settings(&dir, BORROWED_AGENT_SETTINGS);
    write_borrowed_agent_template(&dir);
    let machine_before = read(&root.join("states.yaml"));

    let placed = run_into(&["instantiate", "polish", "--into", "release"], &dir);
    assert!(
        !placed.status.success(),
        "with no project to resolve it, the borrowed agent is unknown; got:\nstdout:\n{}\nstderr:\n{}",
        placed.stdout,
        placed.stderr
    );
    assert_stderr_contains(&placed, "borrowed");
    assert_eq!(machine_before, read(&root.join("states.yaml")), "the target is left alone");
}
