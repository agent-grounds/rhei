//! The shape rule's gate: the rule is stated once, every authoring skill
//! carries that one copy rather than a paraphrase of it, every rule a skill
//! states has a runnable pair behind it, and each pair's README quotes the
//! output both of its runs produce.
//!
//! This is the acceptance of agent-grounds/rhei#324 ported from the
//! reproducer that validation left: the question *state, task or subtask?* had
//! no stated answer, two shipped skills answered it opposite ways, and nothing
//! in `examples/` showed either answer running. Each test here failed for that
//! absence, and each now runs in the suite, so a README that stops quoting its
//! own run, or a skill that paraphrases the rule, fails the commit gate.
// §FS-rhei-shape §FS-rhei-install-skills.4.8 §FS-rhei-run-report.3.2

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::shape_example_terminal_tests::run_on_a_terminal;
use super::*;

/// Every pair under `examples/shape/`, each with the directory names of its two
/// shapes, the one the rule picks first. The one extract every authoring skill
/// ships links every pair here, so a pair joins this list and those links in
/// the same commit; a row of the decision table with no pair stays normative
/// in the spec alone.
const PAIRS: &[(&str, [&str; 2])] = &[
    ("reproducer", ["flat", "nested"]),
    ("review-against-spec", ["nested", "flat"]),
    ("cve-category", ["nested", "flat"]),
    ("parts-of-a-feature", ["flat", "nested"]),
    ("spec-first", ["task", "state"]),
    ("review-rounds", ["loop", "tasks"]),
    ("claiming-the-issue", ["sibling", "root-state"]),
    ("waiting-on-a-person", ["state", "task"]),
    ("candidate-lookup", ["state", "task"]),
    ("running-the-gate", ["task", "state"]),
    ("draft-pull-request", ["state", "task"]),
];

/// Every skill that states the rule, and so must carry the extract and the
/// links. `rhei-plan-worker` reads plans rather than authoring them.
const AUTHORING_SKILLS: [&str; 3] =
    ["rhei-plan-writer", "rhei-state-machine-writer", "rhei-template-writer"];

/// The heading the extract's links sit under; everything above it is the rule's.
const LINKS_HEADING: &str = "## The paired examples";

/// What tells corollary 3 of §FS-rhei-shape.2 from corollary 1.
const COROLLARY_3_QUESTION: &str = "could the product be wrong while the task's outcome is right?";

/// Where the memory test is restated in one sentence, and words that find it.
const RESTATEMENTS: [(&str, &str); 4] = [
    ("docs/functional-spec/rhei-authoring.spec.md", "Ask who reads what the"),
    ("crates/rhei-cli/skills/rhei-plan-writer/SKILL.md", "If anyone but the next state"),
    ("crates/rhei-cli/skills/rhei-template-writer/SKILL.md", "ask who reads what it writes"),
    (
        "crates/rhei-cli/skills/rhei-template-writer/references/pattern-library.md",
        "a product only the next state",
    ),
];

/// The marker comments that delimit a README block this gate owns, named for
/// the shape whose run it quotes. The prose around them is the author's; the
/// bytes between them are the renderer's.
fn markers(block: &str, shape: &str) -> (String, String) {
    (format!("<!-- rhei:{block} {shape} -->"), format!("<!-- /rhei:{block} {shape} -->"))
}

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

/// (1) The rule has one normative copy and every authoring skill carries that
/// copy, byte for byte, rather than a paraphrase that drifts. Four surfaces
/// stating one rule in four wordings is how #324 happened.
// §FS-rhei-shape.7 §FS-rhei-shape.2 §FS-rhei-install-skills.4.8
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
    // verbatim, so editing the spec is the only way to change the rule. Only a
    // link's target may differ, because each copy links from where it lives.
    // A numbered block is §2's corollaries; the numbered links below the rule
    // are the extract's own.
    let (rule, _links) = first.split_once(LINKS_HEADING).expect("the extract ends with its links");
    let spec_text = link_targets_by_name(&spec);
    for block in rule.split("\n\n").filter(|block| block.len() > 80) {
        if ["- ", "| ", "> "].iter().any(|lead| block.starts_with(lead)) || numbered(block) {
            assert!(
                spec_text.contains(&link_targets_by_name(block.trim_end())),
                "the extract may not say what §FS-rhei-shape does not:\n{block}"
            );
        }
    }

    // And it carries the corollaries rather than only being allowed them: the
    // test's one line alone answers "task" for rows its own table rules a state.
    let memory_test = spec.split("\n## ").find(|s| s.starts_with("2. ")).expect("the memory test");
    let corollaries: Vec<&str> =
        memory_test.split("\n\n").skip(1).filter(|b| numbered(b)).collect();
    assert!(!corollaries.is_empty(), "§FS-rhei-shape.2 numbers its corollaries");
    let extracted = link_targets_by_name(rule);
    for block in corollaries {
        assert!(
            extracted.contains(&link_targets_by_name(block.trim_end())),
            "the extract leaves out a corollary of §FS-rhei-shape.2:\n{block}"
        );
    }
    for (skill, text) in &copies {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            text.contains(COROLLARY_3_QUESTION),
            "{skill}: references/shape.md does not ask {COROLLARY_3_QUESTION:?}"
        );
    }
}

/// Does this block open a numbered list item, `1. …`?
fn numbered(block: &str) -> bool {
    block.split_once(". ").is_some_and(|(n, _)| n.parse::<u32>().is_ok())
}

/// `text` with every Markdown link target cut to its last path segment, the
/// file name and anchor, so a link the spec writes from `docs/functional-spec/`
/// and the same link a skill writes from its `references/` compare equal while
/// the link text and every other byte still compare exactly.
fn link_targets_by_name(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        let (head, tail) = rest.split_at(at + 2);
        out.push_str(head);
        let end = tail.find(')').unwrap_or(tail.len());
        out.push_str(tail[..end].rsplit('/').next().unwrap_or_default());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// (2) The two instructions that contradicted each other are gone, and the
/// phase rule carries its qualifier in the skill as well as in the spec. One
/// of these shipped in the same binary as the other. Every restatement of the
/// test also keeps a task's own outcome a state, or a shipped surface still
/// tells an author to make the step that opens the pull request a task.
// §FS-rhei-shape.2 §FS-rhei-state-machine-writer.3.1 §FS-rhei-authoring.3
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
        assert!(
            rule.contains("own outcome"),
            "{label}: the phase rule still makes a task of its task's own outcome: {rule}"
        );
    }
    for (path, words) in RESTATEMENTS {
        let text = fs::read_to_string(repo_root().join(path)).expect(path);
        let at = text.find(words).unwrap_or_else(|| panic!("{path} restates the memory test"));
        // The paragraph or list item the restatement sits in, and nothing past it.
        let start =
            [text[..at].rfind("\n\n"), text[..at].rfind("\n- ")].into_iter().flatten().max();
        let rest = &text[at..];
        let end = [rest.find("\n\n"), rest.find("\n- ")].into_iter().flatten().min();
        let restated = &text[start.map_or(0, |i| i + 1)..at + end.unwrap_or(rest.len())];
        assert!(
            restated.contains("own outcome"),
            "{path}: the restated memory test has no \"own outcome\" clause:\n{restated}"
        );
    }

    // A rule stated in a skill without a link to its example is not finished.
    for skill in AUTHORING_SKILLS {
        let text = fs::read_to_string(skills_root().join(skill).join("SKILL.md")).expect("skill");
        let reference = fs::read_to_string(skills_root().join(skill).join("references/shape.md"))
            .expect("extract");
        for (pair, _) in PAIRS {
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
    for (pair, shapes) in PAIRS {
        for shape in shapes {
            let source = pair_dir(pair, shape);
            assert!(source.is_dir(), "examples/shape/{pair}/{shape} is missing");
            // A scratch copy, because the CLI keeps its home beside the plan
            // and the checkout must stay clean after a suite run.
            let scratch = unique_scratchpad_dir(&format!("shape-validate-{pair}-{shape}"));
            let dir = scratch.join(shape);
            copy_dir_recursive(&source, &dir);
            let result = run_cli_without_machine("validate", &dir, &[]);
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

/// (4) `PAIRS` is what the gate runs and the skills' links are checked
/// against, so a pair on disk it does not list would ship a README nothing
/// compares with its run, and a listed pair or shape with no directory would
/// be a link to nothing.
// §FS-rhei-shape.7
#[test]
fn the_shape_pairs_on_disk_are_the_pairs_listed() {
    let subdirectories = |dir: &Path| -> BTreeSet<String> {
        fs::read_dir(dir)
            .unwrap_or_else(|err| panic!("{} is readable: {err}", dir.display()))
            .map(|entry| entry.expect("directory entry"))
            .filter(|entry| entry.file_type().expect("file type").is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    };
    let listed: BTreeSet<String> = PAIRS.iter().map(|(pair, _)| pair.to_string()).collect();
    assert_eq!(
        subdirectories(&repo_root().join("examples/shape")),
        listed,
        "examples/shape/ and PAIRS must name the same pairs"
    );
    for (pair, shapes) in PAIRS {
        let declared: BTreeSet<String> = shapes.iter().map(|shape| shape.to_string()).collect();
        assert_eq!(
            subdirectories(&pair_root(pair)),
            declared,
            "examples/shape/{pair}/ and its PAIRS entry must name the same shapes"
        );
    }
}

/// (5) What a later task is told and what a person sees, for both shapes. Each
/// shape runs once on a terminal, because the console task tree is the rich
/// summary `rhei run` writes only there (§FS-rhei-run-report.3.4); a probe task
/// is then added to the same workspace, and `rhei next --peek` renders the Plan
/// History it would be told. Both are compared byte for byte with the README's
/// blocks for that shape, so neither the ruled shape's output nor the compared
/// one's can drift from what it renders.
// §FS-rhei-memory.3.2 §FS-rhei-run-report.3.2 §REQ-cross-platform.2
#[test]
fn a_pair_readme_quotes_what_both_its_runs_render() {
    for (pair, shapes) in PAIRS {
        let readme = fs::read_to_string(pair_root(pair).join("README.md")).expect("README");
        for shape in shapes {
            let dir = unique_scratchpad_dir(&format!("shape-{pair}-{shape}"));
            let workspace = dir.join(shape);
            copy_dir_recursive(&pair_dir(pair, shape), &workspace);

            let transcript = run_on_a_terminal(&workspace);
            let tree = transcript
                .split("\nTasks")
                .nth(1)
                .map(|rest| rest.split("\n\n").next().unwrap_or(rest))
                .unwrap_or_else(|| {
                    panic!(
                        "{pair}/{shape}: the terminal summary carries a task tree:\n{transcript}"
                    )
                });
            let tree = normalize(tree, &workspace);
            if *shape == "nested" {
                assert!(
                    tree.contains("subtasks:"),
                    "{pair}/{shape}: a finished parent speaks for its subtree on the console \
                     tree; got:\n{tree}"
                );
            }
            let (open, close) = markers("task-tree", shape);
            let quoted = marked_block(&readme, &open, &close).unwrap_or_else(|| {
                panic!("{pair}: README quotes the {shape} task tree between {open} and {close}")
            });
            assert_eq!(
                unfenced(&quoted),
                tree.trim_matches('\n'),
                "{pair}: the README's {shape} task tree is not what the run renders"
            );

            // The run finished every task, and a finished plan has no next task
            // to peek; one open task added after it is the reader the history is for.
            fs::write(
                workspace.join("tasks/99-probe.md"),
                "### Task 99: Read what came before\n**State:** work\n\n\
                 A task added after the run, so the run's history has a reader.\n",
            )
            .expect("add the probe task");
            let peeked = run_cli_without_machine("next", &workspace, &["--peek"]);
            assert_success(&peeked);
            let history = peeked
                .stdout
                .split("## Plan History")
                .nth(1)
                .map(|rest| {
                    rest.split("\n## ").next().unwrap_or(rest).trim_matches('\n').trim_end()
                })
                .unwrap_or_else(|| {
                    panic!("{pair}/{shape}: the peeked prompt carries Plan History")
                });
            let (open, close) = markers("plan-history", shape);
            let quoted = marked_block(&readme, &open, &close).unwrap_or_else(|| {
                panic!("{pair}: README quotes the {shape} Plan History between {open} and {close}")
            });
            assert_eq!(
                unfenced(&quoted),
                normalize(history, &workspace),
                "{pair}: the README's {shape} Plan History is not what the run renders"
            );
        }
    }
}
