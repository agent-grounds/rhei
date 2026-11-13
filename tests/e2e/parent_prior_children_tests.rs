// A `**Prior:**` on a parent gates that parent's own visit and nothing beneath
// it, so a child with no `**Prior:**` of its own is ready beside the parent's
// prerequisite. The fact is pinned first, as `rhei list --ready` reports it,
// and then the two writer skills that must tell an author so, read the way an
// installed `rhei` hands them out (agent-grounds/rhei#692).

// §FS-rhei-plan-language.3.1 §FS-rhei-list.3.1 §FS-rhei-install-skills.4.3

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::installed_state_machine_guidance_tests::{
    assert_no_filesystem_skill_source, binary_outside_checkout,
};
use super::*;

/// The issue's plan: the ordering is on Task 2 alone, its children are bare.
const PARENT_PRIOR_PLAN: &str = r#"# Rhei: Parent Prior

## Tasks

### Task 1: Prepare the ground
**State:** pending

### Task 2: Build on the ground
**State:** pending
**Prior:** Task 1

#### Task 2.1: First part
**State:** pending

#### Task 2.2: Second part
**State:** pending
"#;

/// The control: the same plan with Task 1 carried by each child that must wait.
const CHILD_PRIORS_PLAN: &str = r#"# Rhei: Parent Prior

## Tasks

### Task 1: Prepare the ground
**State:** pending

### Task 2: Build on the ground
**State:** pending
**Prior:** Task 1

#### Task 2.1: First part
**State:** pending
**Prior:** Task 1

#### Task 2.2: Second part
**State:** pending
**Prior:** Task 1
"#;

/// The ids `rhei list --ready --json` holds, validated first: the shape is a
/// valid plan, and nothing here asks `rhei validate` to say otherwise.
fn ready_ids(prefix: &str, plan: &str) -> BTreeSet<String> {
    let (_dir, plan_path, machine_path) = setup_single_file(prefix, plan);
    assert_success(&run_cli("validate", &plan_path, &machine_path, &[]));
    let ready = run_cli("list", &plan_path, &machine_path, &["--ready", "--json"]);
    assert_success(&ready);
    serde_json::from_str::<serde_json::Value>(&ready.stdout)
        .expect("parse `rhei list --ready --json`")
        .as_array()
        .expect("a JSON array")
        .iter()
        .map(|task| task["id"].as_str().expect("an id").to_string())
        .collect()
}

fn ids(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|id| id.to_string()).collect()
}

/// What the writer skills' sentence states. If the ready set ever changes,
/// this fails and the sentence is known to be stale.
/// §FS-rhei-plan-language.3.1 §FS-rhei-list.3.1
#[test]
fn issue_692_a_parent_prior_leaves_its_bare_children_ready_at_once() {
    assert_eq!(
        ready_ids("parent-prior-report", PARENT_PRIOR_PLAN),
        ids(&["plan.1", "plan.2.1", "plan.2.2"]),
        "Task 2's Prior gates Task 2 alone: its bare children are ready beside Task 1"
    );
    assert_eq!(
        ready_ids("parent-prior-control", CHILD_PRIORS_PLAN),
        ids(&["plan.1"]),
        "each child that carries the Prior itself waits for Task 1"
    );
}

/// Each writer skill, where it states the opposite-direction rule, and the
/// section that holds it.
const WRITER_SKILLS: [(&str, &str, &str); 2] = [
    ("rhei-plan-writer", "## Validation Checklist\n", "\n## "),
    ("rhei-template-writer", "### Plan skeleton\n", "\n### "),
];

/// The rule that is already there: a child never names its parent or an ancestor.
const ANCESTOR_RULE: &str = "its parent or any ancestor";

/// The clause this issue adds, found by its negated hold; the other phrases are
/// what it must also say. Matched lowercase, with whitespace collapsed.
const CLAUSE_KEY: &str = "does not hold its children";
const CLAUSE_MUST_SAY: [&str; 2] = ["**prior:**", "own visit"];
const CLAUSE_REMEDY: [&str; 2] = ["each child", "every child"];
/// The point the clause states, cited the way the skills cite one.
const CLAUSE_CITES: &str = "§FS-rhei-plan-language.3.1";

fn install_writer_skills(bin: &Path, cwd: &Path) -> TestDir {
    let home = unique_temp_dir("parent-prior-guidance-install");
    let mut cmd = rhei_process_at(bin);
    cmd.env("HOME", &*home).env("XDG_STATE_HOME", home.join("state")).current_dir(cwd).args([
        "install-skills",
        "--agent",
        "claude-code",
        "--skills",
        "rhei-plan-writer,rhei-template-writer",
    ]);
    assert_success(&CliRun::from(&cmd.output().expect("rhei install-skills should run")));
    home
}

/// The section's list items, one string each, continuation lines folded in.
fn list_items(section: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut open = false;
    for line in section.lines() {
        let trimmed = line.trim();
        let starts_item = trimmed.starts_with("- ")
            || trimmed.split_once(". ").is_some_and(|(n, _)| n.parse::<u32>().is_ok());
        if trimmed.is_empty() {
            open = false;
        } else if starts_item || !open {
            items.push(trimmed.to_string());
            open = true;
        } else if let Some(last) = items.last_mut() {
            last.push(' ');
            last.push_str(trimmed);
        }
    }
    items.into_iter().map(|item| item.split_whitespace().collect::<Vec<_>>().join(" ")).collect()
}

fn assert_parent_prior_guidance(skill_name: &str, skill: &str, heading: &str, end: &str) {
    let section = skill
        .split_once(heading)
        .unwrap_or_else(|| panic!("installed {skill_name} has no `{}`", heading.trim()))
        .1;
    let section = section.split(end).next().expect("the section exists");
    let items = list_items(section);
    let at = |phrase: &str| items.iter().position(|item| item.to_lowercase().contains(phrase));

    let rule = at(ANCESTOR_RULE).unwrap_or_else(|| {
        panic!("installed {skill_name} lost the rule naming {ANCESTOR_RULE:?}:\n{section}")
    });
    let clause = at(CLAUSE_KEY).unwrap_or_else(|| {
        panic!(
            "installed {skill_name} never says a parent's `**Prior:**` {CLAUSE_KEY:?}; its \
             section:\n{section}"
        )
    });
    assert!(
        rule.abs_diff(clause) <= 1,
        "installed {skill_name} says it, but not beside the ancestor rule:\n{section}"
    );
    assert!(
        items[clause].contains(CLAUSE_CITES),
        "{skill_name}'s clause must cite {CLAUSE_CITES}:\n{}",
        items[clause]
    );
    let text = items[clause].to_lowercase();
    for phrase in CLAUSE_MUST_SAY {
        assert!(text.contains(phrase), "{skill_name}'s clause must say {phrase:?}:\n{text}");
    }
    assert!(
        CLAUSE_REMEDY.iter().any(|phrase| text.contains(phrase)),
        "{skill_name}'s clause must tell the author to give each child that must wait its own \
         `**Prior:**`:\n{text}"
    );
}

/// Both writer skills, as a `cargo install`ed `rhei` writes them, say a
/// parent's `**Prior:**` gates only its own visit and that each child that must
/// wait carries it, beside the unchanged rule that a child never names its
/// parent. §FS-rhei-plan-language.3.1 §FS-rhei-install-skills.4.3
#[test]
#[ignore = "red until #692: the writer skills do not say a parent's Prior leaves its children unordered; implement removes this"]
fn issue_692_writer_skills_say_a_parent_prior_does_not_hold_its_children() {
    let bin_dir = unique_temp_dir("parent-prior-guidance-bin");
    let cwd = unique_temp_dir("parent-prior-guidance-cwd");
    let bin = binary_outside_checkout(&bin_dir);
    assert_no_filesystem_skill_source(&bin);
    assert_no_filesystem_skill_source(&cwd);

    let home = install_writer_skills(&bin, &cwd);
    for (name, heading, end) in WRITER_SKILLS {
        let skill = fs::read_to_string(home.join(format!(".claude/skills/{name}/SKILL.md")))
            .unwrap_or_else(|err| panic!("read the installed {name}: {err}"));
        assert_parent_prior_guidance(name, &skill, heading, end);
    }
}
