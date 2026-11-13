// agent-grounds/rhei#670: a codex-family state that inherits a session through
// codex's own `exec fork <id>` was spawned `… fork <id> --`, and codex reads a
// piped prompt there only when its PROMPT is `-`. With none it forks, starts no
// turn and exits 0, so the state ran having been asked nothing.

// §FS-rhei-agents.2.2.2 §FS-rhei-snapshots.9.3.4 §FS-rhei-snapshots.10.1

// CI has no codex, so the agent below is the argv-recording agent of
// `snapshot_codex_session_tests` with codex's PROMPT rule put in front of it.
// The rule lives here and not in that file's shared body, so every other test
// there keeps the agent it was written against.

use std::path::Path;

use super::snapshot_codex_session_tests::{
    argv_of, index_of, recorded_invocations, write_settings, ARGV_AGENT_BODY, TWO_STATE_PLAN,
};
use super::*;

/// Codex's PROMPT rule, as `codex exec`, `exec resume <id>` and `exec fork <id>`
/// apply it (codex-cli 0.159.3 to 0.167.1):
///
/// - a PROMPT of `-` reads the prompt from stdin;
/// - no PROMPT reads stdin too, except under `fork <id>`, which forks the
///   session, starts no turn, reads nothing and exits 0;
/// - a second positional is a usage error, exit 2.
///
/// PROMPT is the first argument after `--`, or a bare `-` on a line with no
/// `--`. A literal prompt is not something rhei hands a `stdin_prompt` profile,
/// so the stand-in refuses one loudly rather than modelling it.
///
/// It runs ahead of [`ARGV_AGENT_BODY`] and sets that body's `READ_STDIN`. The
/// fork-only branch records the invocation the body would have recorded, with
/// the empty stdin codex read, and exits before the body writes a result.
/// Nothing in here is formatted, so a `{` is safe.
const CODEX_PROMPT_RULE: &str = r#"
import json

_args = sys.argv[1:]
if '--' in _args:
    _options = _args[:_args.index('--')]
    _positionals = _args[_args.index('--') + 1:]
else:
    _options = [arg for arg in _args if arg != '-']
    _positionals = [arg for arg in _args if arg == '-']
if len(_positionals) > 1:
    sys.stderr.write('error: unexpected argument ' + repr(_positionals[1]) + ' found\n')
    sys.exit(2)
PROMPT = _positionals[0] if _positionals else None
if PROMPT is None and 'fork' in _options:
    append(pathlib.Path(ARGV_LOG), json.dumps({
        'state': env('RHEI_STATE'),
        'session': '',
        'stdin': '',
        'result_path': '',
        'argv': _args,
        'fork_only': True,
    }) + '\n')
    sys.exit(0)
if PROMPT not in (None, '-'):
    sys.stderr.write('this stand-in reads its prompt from stdin only; PROMPT was ' + repr(PROMPT) + '\n')
    sys.exit(2)
READ_STDIN = 1
"#;

/// The argv-recording agent with codex's PROMPT rule in front of it, writing
/// its transcripts where a codex-shaped layout finds them.
fn write_codex_rule_agent(dir: &Path, sessions_root: &Path) -> PathBuf {
    fn quoted(path: &Path) -> String {
        serde_json::to_string(&path.display().to_string()).expect("path json")
    }
    let preamble = format!(
        "ARGV_LOG = {}\nNESTED_ROOT = {}\nFLAT_DIR = ''\n",
        quoted(&dir.join("agent-argv.jsonl")),
        quoted(sessions_root),
    );
    write_python_agent(dir, "codex.py", &format!("{preamble}{CODEX_PROMPT_RULE}{ARGV_AGENT_BODY}"))
}

/// The issue's profile: `family: codex`, so it ends a stdin line as codex does,
/// and a session block whose resume slot names `strategy` — `fork` to reach
/// codex's own fork, `resume` for the built-in's resume.
fn write_codex_family_settings(root: &Path, agent: &Path, sessions_root: &Path, strategy: &str) {
    write_settings(
        root,
        &format!(
            r#"{{
  "agents": {{
    "cdx": {{
      "family": "codex",
      "command": {},
      "model_flag": "--model",
      "stdin_prompt": true,
      "timeout": "20s",
      "modes": {{"ro": ["--sandbox", "read-only", "--skip-git-repo-check"]}},
      "session": {{
        "resume": {{"flag": {}}},
        "layout": {{
          "kind": "FlatById",
          "dir_template": {},
          "ext": "jsonl",
          "nested": true,
          "id_from_stem": "trailing_uuid",
          "confirm_cwd_path": ["payload", "cwd"]
        }}
      }}
    }}
  }}
}}"#,
            fixture_command(agent),
            serde_json::to_string(strategy).expect("json"),
            serde_json::to_string(&sessions_root.display().to_string()).expect("json"),
        ),
    );
}

/// `source` is a fresh codex run that emits `impl`; `review` inherits it and
/// cannot run without it.
const INHERITING_MACHINE: &str = r#"name: codex-inherit-prompt
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: cdx[ro]:acme:model-a
    snapshot:
      emit:
        name: impl
        on: always
  review:
    description: Consume the implementation snapshot
    target: cdx[ro]:acme:model-a
    snapshot:
      inherit:
        name: impl
        required: true
        select:
          state: source
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: review
  - from: review
    to: completed
"#;

/// What one run of the inheriting plan left behind.
struct InheritingRun {
    run: CliRun,
    source_session: String,
    reviews: Vec<serde_json::Value>,
}

fn run_inheriting_plan(prefix: &str, strategy: &str) -> InheritingRun {
    let dir = unique_temp_dir(prefix);
    let sessions_root = dir.join("codex-sessions");
    let agent = write_codex_rule_agent(&dir, &sessions_root);
    write_codex_family_settings(&dir, &agent, &sessions_root, strategy);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TWO_STATE_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", INHERITING_MACHINE);

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);

    let records = recorded_invocations(&dir);
    let source = records.iter().find(|r| r["state"] == "source").unwrap_or_else(|| {
        panic!("fixture: the source state never ran; records: {records:?}\n{}", run.stderr)
    });
    let source_session = source["session"].as_str().expect("source session id").to_string();
    let reviews: Vec<serde_json::Value> =
        records.iter().filter(|r| r["state"] == "review").cloned().collect();
    assert!(
        !reviews.is_empty(),
        "fixture: review was never spawned, so nothing about its prompt is shown; records: \
         {records:?}\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    InheritingRun { run, source_session, reviews }
}

/// Every `review` spawn read its own prompt, and the subcommand that carried the
/// session sat ahead of the separator with the source's id.
fn assert_every_review_read_its_prompt(outcome: &InheritingRun, subcommand: &str) {
    for review in &outcome.reviews {
        let argv = argv_of(review);
        let stdin = review["stdin"].as_str().unwrap_or_default();
        assert!(
            stdin.contains("## State: review"),
            "review's prompt never reached the agent: codex read no PROMPT on this line and \
             {}; argv: {argv:?}",
            if review["fork_only"] == true {
                "exited 0 having been asked nothing"
            } else {
                "read something else"
            },
        );
        let at = index_of(&argv, subcommand);
        assert_eq!(
            argv.get(at + 1),
            Some(&outcome.source_session),
            "{subcommand} carries the source session's id: {argv:?}"
        );
        assert!(at < index_of(&argv, "--"), "{subcommand} precedes the separator: {argv:?}");
    }
    assert_success(&outcome.run);
}

/// The issue's report: `"resume": {"flag": "fork"}` reaches codex's fork, and
/// the inheriting state must still be asked what it is there to do. Today it is
/// spawned `… fork <id> --`, forks, exits 0 unasked, writes no result and halts.
/// §FS-rhei-agents.2.2.2 §FS-rhei-snapshots.9.3.4
#[test]
fn a_codex_fork_through_the_resume_slot_reads_the_inheriting_states_prompt() {
    let outcome = run_inheriting_plan("snapshot-codex-fork-prompt", "fork");

    assert_every_review_read_its_prompt(&outcome, "fork");
    for review in &outcome.reviews {
        let argv = argv_of(review);
        assert_eq!(
            argv[argv.len().saturating_sub(2)..],
            ["--", "-"],
            "a codex line ends `-- -`: {argv:?}"
        );
    }
}

/// The guard: the same plan through codex's resume, which reads piped stdin
/// with no PROMPT and with `-` alike, so it delivers before the change and
/// after it. Its argv tail is pinned by the fork case and the unit tests, not
/// here, so this stays green throughout. §FS-rhei-snapshots.9.3.4
#[test]
fn a_codex_resume_reads_the_inheriting_states_prompt() {
    let outcome = run_inheriting_plan("snapshot-codex-resume-prompt", "resume");

    assert_every_review_read_its_prompt(&outcome, "resume");
}
