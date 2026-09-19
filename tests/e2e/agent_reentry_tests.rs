//! Fresh work after an agent state is entered again, and the reuse boundaries
//! that distinguish a new visit from a restart of the current one.

// §FS-rhei-agents.3.2 §FS-rhei-agents.8.4 §FS-rhei-transitions.4.3

use std::fs;

use super::agent_reentry_support::*;
use super::*;

#[test]
fn a_manual_reentry_spawns_before_advancing_on_a_static_output() {
    let (dir, plan, machine) = setup(
        "agent-manual-reentry",
        BASIC_MACHINE,
        r#"root = pathlib.Path(env('RHEI_ROOT'))
count_file = root / 'runtime' / 'spawn-count.log'
count = len(count_file.read_text(encoding='utf-8').splitlines()) + 1 if count_file.exists() else 1
append(count_file, 'work {}\n'.format(count))
write(root / 'runtime' / 'digest.md', 'digest from invocation {}\n'.format(count))
"#,
    );
    let args = ["--no-tui", "--no-callbacks"];

    assert_success(&run_cli("run", &plan, &machine, &args));
    assert_eq!(spawn_lines(&dir), ["work 1"]);
    assert_success(&run_transition(&plan, &machine, "1", "verifying", "work"));
    assert_success(&run_cli("run", &plan, &machine, &args));

    let observed = spawn_lines(&dir);
    let transitions = ledger(&dir);
    assert_eq!(
        observed,
        ["work 1", "work 2"],
        "a re-entered visit must run before its second work -> verifying move; \
         observed spawns={observed:?}\ntransition ledger:\n{transitions}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("runtime/digest.md")).expect("digest"),
        "digest from invocation 2\n"
    );
}

const AUTOMATIC_LOOP_MACHINE: &str = r#"name: automatic-agent-reentry
version: 1
states:
  work:
    initial: true
    description: Produce the digest twice
    visits: 2
    agent: mock
    agent_timeout: 10s
    outputs:
      - name: digest
        path: runtime/digest.md
  review:
    description: Return the ticket for another pass
    agent: mock
    agent_timeout: 10s
    outputs:
      - name: review
        path: runtime/review.md
  completed:
    description: Done
    final: true
transitions:
  - { from: work, to: completed, description: Two visits complete, condition: visitCount >= visits }
  - { from: work, to: review, description: Review the first visit, condition: visitCount < visits }
  - { from: review, to: work, description: Request another visit }
"#;

#[test]
fn an_automatic_loop_back_spawns_each_visit_before_completion() {
    let (dir, plan, machine) = setup(
        "agent-automatic-reentry",
        AUTOMATIC_LOOP_MACHINE,
        r#"root = pathlib.Path(env('RHEI_ROOT'))
state = env('RHEI_STATE')
append(root / 'runtime' / 'spawn-count.log', state + '\n')
if state == 'work':
    write(root / 'runtime' / 'digest.md', 'work visit {}\n'.format(env('RHEI_VISIT_COUNT')))
    result('work visit {} completed\n'.format(env('RHEI_VISIT_COUNT')))
else:
    write(root / 'runtime' / 'review.md', 'reviewed\n')
"#,
    );

    let run = run_cli("run", &plan, &machine, &["--parallel", "2", "--no-tui", "--no-callbacks"]);
    assert_success(&run);
    assert_task_state(&plan, &machine, "1", "completed");
    let observed = spawn_lines(&dir);
    let transitions = ledger(&dir);
    assert_eq!(
        observed.iter().filter(|line| line.as_str() == "work").count(),
        2,
        "the automatic work -> review -> work loop requires a worker in both visits; \
         observed spawns={observed:?}\ntransition ledger:\n{transitions}"
    );
}

#[test]
fn a_first_visit_may_reuse_a_deliberately_preseeded_output() {
    let (dir, plan, machine) = setup("agent-initial-preseed", BASIC_MACHINE, COUNTING_AGENT);
    seed_output(&dir, "runtime/digest.md");

    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));
    assert!(spawn_lines(&dir).is_empty(), "an initial preseed must still avoid a spawn");
    assert_task_state(&plan, &machine, "1", "verifying");
}

#[test]
fn a_visit_templated_output_still_resolves_for_each_counted_visit() {
    let machine_text = BASIC_MACHINE
        .replace("    agent: mock\n", "    visits: 2\n    agent: mock\n")
        .replace("path: runtime/digest.md", "path: runtime/digest-{visit_count}.md");
    let (dir, plan, machine) = setup(
        "agent-visit-output",
        &machine_text,
        r#"root = pathlib.Path(env('RHEI_ROOT'))
visit = env('RHEI_VISIT_COUNT')
append(root / 'runtime' / 'spawn-count.log', 'work ' + visit + '\n')
write(root / 'runtime' / ('digest-' + visit + '.md'), 'visit ' + visit + '\n')
"#,
    );

    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));
    assert_success(&run_transition(&plan, &machine, "1", "verifying", "work"));
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));
    assert_eq!(spawn_lines(&dir), ["work 1", "work 2"]);
    assert!(dir.join("runtime/digest-1.md").exists());
    assert!(dir.join("runtime/digest-2.md").exists());
}
