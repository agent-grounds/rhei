// agent-grounds/rhei#325, driven end to end: a fact one finished ticket found
// reaches a later ticket that never declared it as a prior, and the two
// tickets that already receive it are not disturbed on the way.
//
// This is the reproducer `runtime/repro/rhei-325/run.sh` ported into the
// project's own framework. Its four cases are the same four, in the same
// fixture, and the two it already passed are kept as controls rather than
// dropped: the point of the change is that they do not move.

// §FS-rhei-memory.3.1 §FS-rhei-memory.4.2 §FS-rhei-note

use std::fs;

use super::note_store_support::*;
use super::*;

/// §FS-rhei-memory.3.1: the ticket the issue's title is about — the next one
/// in the same rhei, with no `**Prior:**` on the ticket that found the fact.
#[test]
fn a_later_ticket_in_the_same_rhei_is_told_what_its_predecessor_found() {
    let (_dir, root) = note_fixture("note-sibling", Some(&store_contents()));
    assert_success(&run_note_fixture(&root));

    let prompt = note_prompt(&root, "auth.3");
    // The line Plan History already carried, which reads like a finished
    // report and is why nobody opened the result file. §FS-rhei-memory.4.3
    assert!(prompt.contains(SLICE), "got:\n{prompt}");
    assert!(
        prompt.contains(&expected_block()),
        "the fact below that first line must reach auth.3; got:\n{prompt}"
    );
}

/// §FS-rhei-note.3.1: the store is project-level, so a ticket in a rhei that
/// never named `auth.2` is told too. Half the ticket if this does not hold.
#[test]
fn a_ticket_in_an_unrelated_rhei_is_told_the_same_fact() {
    let (_dir, root) = note_fixture("note-unrelated", Some(&store_contents()));
    assert_success(&run_note_fixture(&root));

    let prompt = note_prompt(&root, "reporting.1");
    // auth.2 is named nowhere else in this prompt: no prior edge, no history
    // line, only the execution-root map. §FS-rhei-memory.4.3
    assert!(!prompt.contains(SLICE), "got:\n{prompt}");
    assert!(prompt.contains(&expected_block()), "got:\n{prompt}");
}

/// §FS-rhei-memory.3.1: the block goes inside `## Position`, immediately after
/// `### Project Context`, and nowhere else.
#[test]
fn the_block_sits_inside_position_after_the_project_context() {
    let (_dir, root) = note_fixture("note-placement", Some(&store_contents()));
    assert_success(&run_note_fixture(&root));

    let prompt = note_prompt(&root, "auth.3");
    let notes = prompt.find("### Project Notes").expect("the block is composed");
    let project_context = prompt.find("### Project Context").expect("### Project Context");
    let instructions = prompt.find("## Instructions").expect("## Instructions");
    assert!(project_context < notes, "the block follows the project context; got:\n{prompt}");
    assert!(notes < instructions, "the block is inside `## Position`; got:\n{prompt}");
}

/// §FS-rhei-memory.3.1: a project that never calls the verb composes what it
/// composes today. A control — it passes now and must keep passing.
#[test]
fn a_project_with_no_store_composes_no_block() {
    let (_dir, root) = note_fixture("note-empty", None);
    assert_success(&run_note_fixture(&root));

    for task in ["auth.3", "billing.1", "reporting.1", "mentor.1.2"] {
        let prompt = note_prompt(&root, task);
        assert!(!prompt.contains("### Project Notes"), "{task} got:\n{prompt}");
    }
}

/// §FS-rhei-memory.3.1: a supervising task omits the block exactly as it omits
/// the two context blocks, so every supervisor prompt is unchanged.
#[test]
fn a_supervising_task_gets_no_block() {
    let dir = unique_temp_dir("note-supervisor");
    let project = dir.join("project");
    fs::create_dir_all(project.join("runtime/results")).expect("create project");
    fs::write(
        project.join("index.panta.md"),
        "# Panta: Knowledge\n\n## House Rules\n\nRun the gate before shipping.\n",
    )
    .expect("write manifest");
    fs::write(
        project.join("rhei.rhei.md"),
        "# Rhei: Supervision\n\n## Ground Rules\n\nBrief one step at a time.\n\n## Tasks\n\n\
         ### Task 1: Brief the selected step\n**State:** guiding\n\nBrief exactly one step.\n\n\
         #### Task 1.1: Selected step\n**State:** completed\n",
    )
    .expect("write rhei");
    write_fixture_file(&project, "runtime/notes.md", &store_contents());
    write_fixture_file(
        &project,
        "runtime/results/rhei.1.1.md",
        "## Result\n\nThe selected step was already completed.\n",
    );
    let machine_path = write_fixture_file(&project, "states.yaml", SUPERVISING_MACHINE);

    let agent = write_python_agent(
        &project,
        "capture-agent.py",
        r#"prompt = agent_prompt()
root = pathlib.Path(env('RHEI_ROOT'))
write(root / 'runtime' / 'supervisor-prompt.md', prompt)
result('## Result\n\nBriefed one step.\n')
"#,
    );
    let settings_dir = project.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "capture", "agent_timeout": "30s" }},
  "agents": {{ "capture": {{ "command": {}, "stdin_prompt": true, "timeout": "30s" }} }}
}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write settings");

    let mut cmd = isolated_command(&project);
    cmd.arg("--state-machine").arg(&machine_path).arg("run").arg(&project);
    cmd.args(["--no-callbacks", "--no-tui"]);
    assert_success(&CliRun::from(&cmd.output().expect("rhei run should run")));

    let prompt = fs::read_to_string(project.join("runtime/supervisor-prompt.md"))
        .expect("capture the supervisor prompt");
    assert!(prompt.contains("### Reading the rhei"), "got:\n{prompt}");
    assert!(!prompt.contains("### Rhei Context"), "got:\n{prompt}");
    assert!(!prompt.contains("### Project Context"), "got:\n{prompt}");
    assert!(!prompt.contains("### Project Notes"), "got:\n{prompt}");
    // §FS-rhei-memory.3.4: and the store is one path away instead.
    assert!(prompt.contains("- Notes left for later tickets: "), "got:\n{prompt}");
}

/// A supervising state with one already-finished child: enough for one visit
/// that composes a supervisor prompt and finishes.
const SUPERVISING_MACHINE: &str = r#"name: note-supervisor-e2e
version: 1
states:
  guiding:
    initial: true
    description: Brief the selected step
    execute_on: descendant-terminal
    agent: capture
    agent_timeout: 30s
    visits: 5
    instructions: Brief exactly one selected step.
  completed:
    final: true
    description: Finished
  cancelled:
    final: true
    description: Cancelled
transitions:
  - from: guiding
    to: completed
    description: The selected subtree is closed
    condition: openDescendants < 1
  - from: guiding
    to: guiding
    description: Release the selected subtree
  - from: '*'
    to: cancelled
    description: Cancelled
"#;

/// The criterion the plan was approved with: `billing.1`'s prompt differs from
/// the one composed at `315860109b` by the added block and by nothing else.
///
/// `billing.1` declares `**Prior:** Task auth.2`, so it already receives the
/// whole result file under `## Prior Task Results` and reads `see above` in
/// Plan History. Suppressing the block for the best-informed reader would be
/// backwards; leaving anything else about its prompt different would break
/// what the author settled. §FS-rhei-memory.4.3
#[test]
fn a_declared_prior_gains_the_block_and_nothing_else() {
    let (_dir, root) = note_fixture("note-prior", Some(&store_contents()));
    assert_success(&run_note_fixture(&root));

    let prompt = note_prompt(&root, "billing.1");
    // What must not move: the paste, and the slice deferring to it.
    assert!(prompt.contains("## Prior Task Results"), "got:\n{prompt}");
    assert!(prompt.contains(&format!("Trap: {TRAP}")), "got:\n{prompt}");
    assert!(
        prompt.contains(
            "- Task auth.2: Implement token refresh \u{2014} completed \u{2014} see above (rhei \
             `auth`, prior)\n"
        ),
        "got:\n{prompt}"
    );

    assert_only_the_agreed_additions(&root, "billing.1", &prompt);
}

/// The same criterion for the decomposed subtree: `mentor.1.2` keeps the
/// `### Parent` paste it already had, at the 200-line cap, and gains the block.
/// §FS-rhei-memory.4.2
#[test]
fn a_decomposed_child_gains_the_block_and_nothing_else() {
    let (_dir, root) = note_fixture("note-subtree", Some(&store_contents()));
    assert_success(&run_note_fixture(&root));

    let prompt = note_prompt(&root, "mentor.1.2");
    assert!(
        prompt.contains("### Parent: Task mentor.1: Carry the fact down a subtree\n"),
        "got:\n{prompt}"
    );

    assert_only_the_agreed_additions(&root, "mentor.1.2", &prompt);
}

/// The whole prompt, against the capture taken at `315860109b`, with the
/// three things this change is allowed to add cut out of it.
///
/// Byte-for-byte after that, which is the only way to say "and nothing else"
/// about a prompt: a `contains` cannot see what a change dropped. The three
/// are the ones §FS-rhei-memory itself now names — the composed block, and
/// the store's line in the map and the one permitted write in
/// `Leaving a trail`, both of §FS-rhei-memory.3.4. Anything beyond them has
/// moved a prompt this ticket promised not to move.
fn assert_only_the_agreed_additions(root: &Path, task: &str, prompt: &str) {
    let normalized = normalize_prompt(root, prompt);
    let (rest, block) = without_project_notes(&normalized);
    let (rest, map_line) = without_line_starting(&rest, "- Notes left for later tickets: ");
    let (rest, trail_line) =
        without_line_starting(&rest, "- You may leave **one** note for later tickets");

    // The capture first: it is what says "and nothing else", and it has to be
    // readable on the run that writes it.
    let baseline = baseline_prompt(task, &rest);
    assert_eq!(rest, baseline, "{task} differs from the capture by more than the three additions");

    assert_eq!(
        block.as_deref(),
        Some(format!("{}\n", expected_block()).as_str()),
        "{task} must carry exactly the composed block"
    );
    assert_eq!(
        map_line.as_deref(),
        Some("- Notes left for later tickets: `{ROOT}/runtime/notes.md`\n"),
        "{task} must be told where the store is"
    );
    let trail = trail_line.unwrap_or_default();
    assert!(trail.contains("`rhei note \"<fact>\"`"), "{task} got:\n{trail}");
    assert!(trail.contains("Never edit `runtime/notes.md` by hand."), "{task} got:\n{trail}");
}
