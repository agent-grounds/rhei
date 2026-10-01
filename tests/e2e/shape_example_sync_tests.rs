//! The shape rule's gate: the rule is stated once, every authoring skill
//! carries that one copy rather than a paraphrase of it, every rule a skill
//! states has a runnable pair behind it, and each pair's README quotes the
//! output its own run produces.
//!
//! This is the acceptance of agent-grounds/rhei#324 ported from the
//! reproducer that validation left: the question *state, task or subtask?* had
//! no stated answer, two shipped skills answered it opposite ways, and nothing
//! in `examples/` showed either answer running. Every test here fails for that
//! absence and is `#[ignore]`d until it is closed — the commit gate runs the
//! whole suite, so a contract cannot be both committed and red. Remove the
//! attribute with the change that satisfies it.
// §FS-rhei-shape §FS-rhei-install-skills.4.8 §FS-rhei-run-report.3.2

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// The four pairs the skills are allowed to link. The rest of the decision
/// table lives in the spec, where a row needs no example to be normative.
const PAIRS: [&str; 4] =
    ["reproducer", "review-against-spec", "cve-category", "parts-of-a-feature"];

/// Every skill that states the rule, and so must carry the extract and the
/// links. `rhei-plan-worker` reads plans rather than authoring them.
const AUTHORING_SKILLS: [&str; 3] =
    ["rhei-plan-writer", "rhei-state-machine-writer", "rhei-template-writer"];

/// The marker comments that delimit a README block this gate owns. The prose
/// around them is the author's; the bytes between them are the renderer's.
const PROMPT_OPEN: &str = "<!-- rhei:plan-history -->";
const PROMPT_CLOSE: &str = "<!-- /rhei:plan-history -->";
const CONSOLE_OPEN: &str = "<!-- rhei:task-tree -->";
const CONSOLE_CLOSE: &str = "<!-- /rhei:task-tree -->";

fn skills_root() -> PathBuf {
    repo_root().join("crates/rhei-cli/skills")
}

fn shape_spec() -> String {
    fs::read_to_string(repo_root().join("docs/functional-spec/rhei-shape.spec.md"))
        .expect("the rule is stated at docs/functional-spec/rhei-shape.spec.md")
}

/// The block between two marker lines, without the markers, or `None` when the
/// pair is absent.
fn marked_block(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(text[start..end].trim_matches('\n').to_string())
}

/// The fenced body of a block, so a README may wrap the quoted output in a
/// code fence and still be compared to plain rendered text.
fn unfenced(block: &str) -> String {
    let mut lines: Vec<&str> = block.lines().collect();
    if lines.first().is_some_and(|line| line.trim_start().starts_with("```")) {
        lines.remove(0);
    }
    if lines.last().is_some_and(|line| line.trim() == "```") {
        lines.pop();
    }
    lines.join("\n").trim_end().to_string()
}

/// What a run writes that a README cannot quote: where it ran, and how long it
/// took. Everything else — ids, titles, states, result summaries, counts — is
/// stable by construction, which is what makes a byte comparison survivable.
fn normalize(text: &str, workspace: &Path) -> String {
    let mut out = text.replace(&workspace.display().to_string(), "<workspace>");
    if let Ok(canonical) = fs::canonicalize(workspace) {
        out = out.replace(&canonical.display().to_string(), "<workspace>");
    }
    out.lines()
        .map(|line| {
            let mut masked = String::new();
            let mut rest = line;
            while let Some(at) = rest.find(|c: char| c.is_ascii_digit()) {
                masked.push_str(&rest[..at]);
                let tail = &rest[at..];
                let width = tail
                    .find(|c: char| !matches!(c, '0'..='9' | '.' | 'm' | 's'))
                    .unwrap_or(tail.len());
                let token = &tail[..width];
                if token.ends_with('s') && token.chars().any(|c| c.is_ascii_digit()) {
                    masked.push_str("<t>");
                } else {
                    masked.push_str(token);
                }
                rest = &tail[width..];
            }
            masked.push_str(rest);
            masked.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn pair_root(pair: &str) -> PathBuf {
    repo_root().join("examples/shape").join(pair)
}

fn pair_dir(pair: &str, shape: &str) -> PathBuf {
    pair_root(pair).join(shape)
}

/// Copy one shape of one pair into a scratch workspace and run it to the end.
fn run_shape(pair: &str, shape: &str) -> (TestDir, PathBuf) {
    let dir = unique_scratchpad_dir(&format!("shape-{pair}-{shape}"));
    let workspace = dir.join(shape);
    copy_dir_recursive(&pair_dir(pair, shape), &workspace);
    let machine = workspace.join("states.yaml");
    let result = run_cli("run", &workspace, &machine, &["--no-tui"]);
    assert_success(&result);
    (dir, workspace)
}

/// (1) The rule has one normative copy and every authoring skill carries that
/// copy, byte for byte, rather than a paraphrase that drifts. Four surfaces
/// stating one rule in four wordings is how #324 happened.
// §FS-rhei-shape.7 §FS-rhei-install-skills.4.8
#[test]
fn the_shape_rule_is_one_normative_copy_every_skill_extracts() {
    let spec = shape_spec();
    let mut copies = Vec::new();
    for skill in AUTHORING_SKILLS {
        let path = skills_root().join(skill).join("references/shape.md");
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} ships the shared extract: {err}", path.display()));
        copies.push((skill, text));
    }
    let (first_skill, first) = &copies[0];
    for (skill, text) in &copies[1..] {
        assert_eq!(
            text, first,
            "{skill} and {first_skill} must ship byte-identical copies of one extract"
        );
    }

    // The extract is an extract: every one of its quoted blocks is in the spec
    // verbatim, so editing the spec is the only way to change the rule.
    for block in first.split("\n\n").filter(|block| block.len() > 80) {
        if block.starts_with("- ") || block.starts_with("| ") || block.starts_with("> ") {
            assert!(
                spec.contains(block.trim_end()),
                "the extract may not say what §FS-rhei-shape does not:\n{block}"
            );
        }
    }
}

/// (2) The two instructions that contradicted each other are gone, and the
/// phase rule carries its qualifier in the skill as well as in the spec. One
/// of these shipped in the same binary as the other.
// §FS-rhei-shape.2 §FS-rhei-state-machine-writer.3.1
#[test]
fn no_authoring_skill_contradicts_the_shape_rule() {
    let plan_writer =
        fs::read_to_string(skills_root().join("rhei-plan-writer/SKILL.md")).expect("skill");
    for contradiction in
        ["Default to adding child tasks", "has child tasks unless it is clearly simple"]
    {
        assert!(
            !plan_writer.contains(contradiction),
            "rhei-plan-writer still tells the author to nest by default: {contradiction:?}"
        );
    }

    for (label, text) in [
        (
            "the spec",
            fs::read_to_string(
                repo_root().join("docs/functional-spec/rhei-state-machine-writer.spec.md"),
            )
            .expect("spec"),
        ),
        (
            "the skill",
            fs::read_to_string(skills_root().join("rhei-state-machine-writer/SKILL.md"))
                .expect("skill"),
        ),
    ] {
        let rule = text
            .lines()
            .find(|line| {
                line.contains("distinct workflow phase")
                    || line.contains("One state per distinct workflow phase")
            })
            .unwrap_or_else(|| panic!("{label} states the phase rule"));
        assert!(
            rule.contains("unless") || rule.contains("read by"),
            "{label}: the phase rule is still unqualified: {rule}"
        );
    }

    // A rule stated in a skill without a link to its example is not finished.
    for skill in AUTHORING_SKILLS {
        let text = fs::read_to_string(skills_root().join(skill).join("SKILL.md")).expect("skill");
        let reference = fs::read_to_string(skills_root().join(skill).join("references/shape.md"))
            .expect("extract");
        for pair in PAIRS {
            let link = format!("examples/shape/{pair}");
            assert!(
                text.contains(&link) || reference.contains(&link),
                "{skill} states the rule but links no runnable {pair}"
            );
        }
    }
}

/// (3) Each pair ships the same work authored both ways, and both ways are
/// valid plans — a pair that only validates in the shape it argues for proves
/// nothing, because the complaint was never that the other shape is rejected.
// §FS-rhei-shape.4
#[test]
fn every_shape_pair_ships_both_shapes_and_both_validate() {
    for pair in PAIRS {
        for shape in ["flat", "nested"] {
            let dir = pair_dir(pair, shape);
            assert!(dir.is_dir(), "examples/shape/{pair}/{shape} is missing");
            let machine = dir.join("states.yaml");
            let result = run_cli("validate", &dir, &machine, &[]);
            assert_success(&result);
        }
        let readme = pair_root(pair).join("README.md");
        let text = fs::read_to_string(&readme).unwrap_or_else(|err| {
            panic!("{} says which shape the rule picks: {err}", readme.display())
        });
        assert!(
            text.contains("§FS-rhei-shape"),
            "{}: a pair's README cites the rule it illustrates",
            readme.display()
        );
    }
}

/// (4) What a later task is told. The README's Plan History block is compared
/// byte for byte against the prompt the pair's own run renders, so the
/// quoted output cannot drift from the output.
// §FS-rhei-memory.3.2
#[test]
#[ignore = "pins agent-grounds/rhei#324; remove this attribute with the change"]
fn a_pair_readme_quotes_the_plan_history_its_own_run_renders() {
    for pair in PAIRS {
        let (_dir, workspace) = run_shape(pair, "nested");
        let machine = workspace.join("states.yaml");
        let peeked = run_cli("next", &workspace, &machine, &["--peek"]);
        assert_success(&peeked);
        let prompt = peeked.stdout.clone();
        let rendered = prompt
            .split("## Plan History")
            .nth(1)
            .map(|rest| rest.split("\n## ").next().unwrap_or(rest).trim_end())
            .unwrap_or_else(|| panic!("{pair}: the peeked prompt carries Plan History"));

        let readme = fs::read_to_string(pair_root(pair).join("README.md")).expect("README");
        let quoted = marked_block(&readme, PROMPT_OPEN, PROMPT_CLOSE).unwrap_or_else(|| {
            panic!("{pair}: README quotes Plan History between {PROMPT_OPEN} and {PROMPT_CLOSE}")
        });
        assert_eq!(
            unfenced(&quoted),
            normalize(rendered, &workspace),
            "{pair}: the README's Plan History is not what the run renders"
        );
    }
}

/// (5) What a person sees. The console task tree is the rich summary, which
/// `rhei run` writes only to a terminal (§FS-rhei-run-report.3.4), so the run
/// is driven through the suite's `portable-pty` harness — native on Unix,
/// ConPTY on Windows, one case on every supported platform.
// §FS-rhei-run-report.3.2 §REQ-cross-platform.2
#[test]
#[ignore = "pins agent-grounds/rhei#324; remove this attribute with the change"]
fn a_pair_readme_quotes_the_console_tree_its_own_run_renders() {
    for pair in PAIRS {
        let dir = unique_scratchpad_dir(&format!("shape-tree-{pair}"));
        let workspace = dir.join("nested");
        copy_dir_recursive(&pair_dir(pair, "nested"), &workspace);
        let transcript = run_on_a_terminal(&workspace);
        let rendered = transcript
            .split("\nTasks")
            .nth(1)
            .map(|rest| rest.split("\n\n").next().unwrap_or(rest))
            .unwrap_or_else(|| {
                panic!("{pair}: the terminal summary carries a task tree:\n{transcript}")
            });

        let readme = fs::read_to_string(pair_root(pair).join("README.md")).expect("README");
        let quoted = marked_block(&readme, CONSOLE_OPEN, CONSOLE_CLOSE).unwrap_or_else(|| {
            panic!("{pair}: README quotes the task tree between {CONSOLE_OPEN} and {CONSOLE_CLOSE}")
        });
        let rendered = normalize(rendered, &workspace);
        assert!(
            rendered.contains("subtasks:"),
            "{pair}: a finished parent speaks for its subtree on the console tree; got:\n{rendered}"
        );
        assert_eq!(
            unfenced(&quoted),
            rendered.trim_matches('\n'),
            "{pair}: the README's task tree is not what the run renders"
        );
    }
}

/// Run a workspace to the end on a real terminal and return everything it
/// wrote there, so the rich end-of-run summary is reached at all.
fn run_on_a_terminal(workspace: &Path) -> String {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::io::Read;

    let source = rhei_command(workspace.join(".home"));
    let mut command = CommandBuilder::new(source.get_program());
    for (key, value) in source.get_envs() {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        }
    }
    command.env("NO_COLOR", "1");
    command.arg("--state-machine");
    command.arg(workspace.join("states.yaml"));
    command.arg("run");
    command.arg(workspace);
    command.arg("--no-dashboard");

    let pair = native_pty_system()
        .openpty(PtySize { rows: 60, cols: 240, pixel_width: 0, pixel_height: 0 })
        .expect("open a pty");
    let mut child = pair.slave.spawn_command(command).expect("spawn the run on a pty");
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().expect("read the pty");
    let transcript = std::thread::spawn(move || {
        let mut text = String::new();
        let mut buffer = [0u8; 4096];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            text.push_str(&String::from_utf8_lossy(&buffer[..count]));
        }
        text
    });
    let status = child.wait().expect("the run returns on its own");
    drop(pair.master);
    let transcript = transcript.join().expect("terminal transcript drained");
    assert_eq!(status.exit_code(), 0, "the example run finishes:\n{transcript}");
    transcript.replace("\r\n", "\n")
}
