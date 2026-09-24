// The no-minting invariant, at the seams a person actually reaches for when a
// run stops: run it again, reset it, narrow it, add to it.
//
// Each of these is the obvious next move after a halt, and each of them would
// be a faucet if it created capacity. That is the whole point: a bound that any
// ordinary command refills is not a bound, and "once per run, forever" is
// exactly the unbounded case the count exists for.

// §REQ-bounded-neural-work.4 §FS-rhei-budgets.11

/// A project whose settings bound it to `moves` travel units and `starts`
/// invocations a day.
///
/// The bounds go in the *project's* settings rather than the machine's because
/// this harness shares one `HOME` across the whole test binary: a machine-tier
/// file would be every other test's machine too. Both numbers are far below the
/// built-in ceilings, so the project tier is honored rather than clamped.
// §FS-rhei-budgets.2 §FS-rhei-agents.1.1
fn budget_project(prefix: &str, moves: u64, starts: u64) -> (TestDir, PathBuf, PathBuf) {
    budget_project_with_agent(prefix, moves, starts, "", r#""stdin_prompt": true"#, "agent: mock")
}

/// [`budget_project`], with the agent's own shape left to the caller: `prologue`
/// runs before the looping body, `profile` is the agent-profile JSON that
/// decides how the run reaches it, and `selector` is the line each working
/// state names its agent with.
fn budget_project_with_agent(
    prefix: &str,
    moves: u64,
    starts: u64,
    prologue: &str,
    profile: &str,
    selector: &str,
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let agent = write_python_agent(
        &dir,
        "looping-agent.py",
        &format!(
            r#"{prologue}root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'spawns.log', '{{}}\n'.format(env('RHEI_STATE')))
write(root / 'runtime' / 'work.md', 'work\n')
write(root / 'runtime' / 'review.md', 'review\n')
"#
        ),
    );
    let settings = dir.join(".agent-grounds").join("rhei");
    fs::create_dir_all(&settings).expect("project settings directory");
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{
    "agent": "mock",
    "agent_timeout": "30s",
    "transition_limit": {moves},
    "invocations_per_day": {starts}
  }},
  "agents": {{ "mock": {{ "command": {}, {profile}, "timeout": "30s" }} }}
}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("project settings");
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: budget-no-minting
version: 1
states:
  work:
    initial: true
    description: A round of work
    {selector}
    agent_timeout: 30s
    outputs:
      - name: work
        path: runtime/work.md
  review:
    description: Another round
    {selector}
    agent_timeout: 30s
    outputs:
      - name: review
        path: runtime/review.md
  cancelled:
    description: Stop
    final: true
transitions:
  - {{ from: work, to: review, description: Round done }}
  - {{ from: review, to: work, description: Another round }}
  - {{ from: work, to: cancelled, description: Stop }}
  - {{ from: review, to: cancelled, description: Stop }}
"#
        ),
    );
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: No minting\n\n## Tasks\n\n### Task 1: Work\n**State:** work\n",
    );
    (dir, plan, machine)
}

/// Every applied move the engine recorded. Travel is counted from these, so
/// this is what a replenishment would show up in.
fn applied_moves(root: &Path) -> usize {
    fs::read_to_string(root.join("runtime/state-transitions.log"))
        .unwrap_or_default()
        .lines()
        .count()
}

fn spawn_count(root: &Path) -> usize {
    fs::read_to_string(root.join("runtime/spawns.log")).unwrap_or_default().lines().count()
}

/// A fresh `rhei run` is the most natural thing to do after a run stops, and
/// the one that must buy nothing. Every fresh run starting both counts again is
/// precisely the failure `agent-grounds/rhei#232` paid for.
// §REQ-bounded-neural-work.4
#[test]
fn a_second_run_of_the_same_project_replenishes_nothing() {
    let (dir, plan, machine) = budget_project("budget-restart", 2, 50);

    run_run_command(&plan, &machine, &["--no-callbacks"]);
    let after_first = applied_moves(&dir);
    assert_eq!(after_first, 2, "the first run spends the project's two travel units");

    let again = run_run_command(&plan, &machine, &["--no-callbacks"]);

    assert_eq!(applied_moves(&dir), 2, "a second run buys no further move");
    assert!(!again.status.success(), "and it is halted rather than quietly idle");
    assert!(
        format!("{}{}", again.stdout, again.stderr).contains("ticket travel"),
        "the halt names the dimension that stopped it\nstdout:\n{}\nstderr:\n{}",
        again.stdout,
        again.stderr
    );
}

/// Reset returns authored state and deletes `runtime/`. It returns no travel:
/// the count is bound to the ticket's identity rather than to one execution,
/// and an inverse that gave it back would make reset the way to buy more.
// §FS-rhei-reset §FS-rhei-budgets.4.1
#[test]
fn reset_and_rerun_converges_on_the_travel_bound() {
    let (dir, plan, machine) = budget_project("budget-reset", 2, 50);
    run_run_command(&plan, &machine, &["--no-callbacks"]);
    assert_eq!(applied_moves(&dir), 2);

    let mut reset = rhei_command();
    reset.arg("--state-machine").arg(&machine).arg("reset").arg(&plan).arg("--yes");
    assert!(reset.output().expect("reset runs").status.success(), "reset succeeds");
    // Reset deleted `runtime/`, so both logs start from nothing: zero is the
    // assertion precisely because the ticket's *identity* kept its history.
    assert_eq!(applied_moves(&dir), 0, "reset deleted the ledger with runtime/");

    let after = run_run_command(&plan, &machine, &["--no-callbacks"]);

    assert_eq!(spawn_count(&dir), 0, "the rerun buys no move at all");
    assert!(!after.status.success(), "and is halted again on the same identity");
}

/// `--rhei` selects candidate tickets. It does not select a balance: there is
/// one account per project whatever the selection, and a narrowing that opened
/// a second would make the flag the faucet.
// §FS-rhei-run.2.5 §FS-rhei-budgets.11
#[test]
fn narrowing_a_run_with_rhei_opens_no_second_balance() {
    let (dir, plan, machine) = budget_project("budget-narrowed", 2, 50);
    run_run_command(&plan, &machine, &["--no-callbacks"]);
    assert_eq!(applied_moves(&dir), 2);

    let narrowed = run_run_command(&plan, &machine, &["--no-callbacks", "--rhei", "plan"]);

    assert_eq!(applied_moves(&dir), 2, "a narrowed run draws on the same spent account");
    assert!(!narrowed.status.success());
}

/// Adding work to a project joins it to the one account. A member that arrived
/// later is still the same project, and joining creates no capacity — otherwise
/// the way past a spent day would be to write another ticket.
// §FS-rhei-budgets.1 §FS-rhei-budgets.11
#[test]
fn appending_a_ticket_to_a_spent_project_creates_no_capacity() {
    let (dir, plan, machine) = budget_project("budget-appended", 50, 2);
    run_run_command(&plan, &machine, &["--no-callbacks"]);
    let spent = spawn_count(&dir);
    assert_eq!(spent, 2, "the day's two invocations are spent");

    // The same plan, with a second ticket appended after the account was
    // already drawn down.
    let body = fs::read_to_string(&plan).expect("plan reads");
    fs::write(&plan, format!("{body}\n### Task 2: More work\n**State:** work\n"))
        .expect("append a ticket");
    let appended = run_run_command(&plan, &machine, &["--no-callbacks"]);

    assert_eq!(
        spawn_count(&dir),
        spent,
        "the new ticket draws on the project's one account, which is spent"
    );
    assert!(
        format!("{}{}", appended.stdout, appended.stderr).contains("project invocations"),
        "and it is the project's day that says so\nstdout:\n{}\nstderr:\n{}",
        appended.stdout,
        appended.stderr
    );
}

/// A project whose agent resolves a provider and a model, which is what makes
/// `rhei run` snapshot a state exit at all: an agent that resolves neither is
/// skipped by auto-emission, and a fixture that is skipped pins nothing.
// §FS-rhei-snapshots.3.1 §FS-rhei-agents.1.1
fn snapshotting_budget_project(
    prefix: &str,
    moves: u64,
    starts: u64,
) -> (TestDir, PathBuf, PathBuf) {
    budget_project_with_agent(
        prefix,
        moves,
        starts,
        r#"session_dir = ''
args = sys.argv[1:]
while args:
    if args.pop(0) == '--session-dir' and args:
        session_dir = args.pop(0)
if not session_dir:
    sys.exit('the agent was spawned without --session-dir')
write(pathlib.Path(session_dir) / 'session.jsonl', '{"provider":"openai","model":"model"}\n')
"#,
        r#""session": { "session_dir_flag": "--session-dir", "layout": { "kind": "FlatById", "ext": "jsonl" } }"#,
        "target: mock:openai:model",
    )
}

/// A snapshot stages a session, not capacity. The run caches one at every
/// agent-state exit it can, under `.rhei/cache/` in the same project root the
/// account lives in — so the cheapest wrong implementation is one that reads a
/// balance back out of a cache the project also writes. The table of
/// §FS-rhei-budgets.11 says a snapshot creates none, and until now nothing
/// pinned that row: the other four seams it names each have a case above.
// §FS-rhei-budgets.11 §REQ-bounded-neural-work.4
#[test]
fn snapshotting_a_spent_project_creates_no_capacity() {
    let (dir, plan, machine) = snapshotting_budget_project("budget-snapshot", 2, 50);

    run_run_command(&plan, &machine, &["--no-callbacks"]);
    assert_eq!(applied_moves(&dir), 2, "the first run spends the project's two travel units");
    // The precondition, asserted rather than assumed: without it this case
    // would pass on a run that snapshotted nothing at all.
    assert!(
        !collect_run_agent_snapshot_manifests(&dir).is_empty(),
        "the run cached a snapshot, so the seam under test was actually reached"
    );

    let again = run_run_command(&plan, &machine, &["--no-callbacks"]);

    assert_eq!(applied_moves(&dir), 2, "the cached session buys no further move");
    assert!(!again.status.success(), "and the rerun is halted rather than quietly idle");
    assert!(
        format!("{}{}", again.stdout, again.stderr).contains("ticket travel"),
        "the halt names the dimension that stopped it\nstdout:\n{}\nstderr:\n{}",
        again.stdout,
        again.stderr
    );
}
