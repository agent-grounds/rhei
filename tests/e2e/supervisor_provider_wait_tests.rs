//! An owed supervisor visit waits on its provider despite its open subtree.
//! §FS-rhei-run.5.1 §FS-rhei-run.3.3 §FS-rhei-supervision.3.2

#[cfg(unix)]
use super::provider_limit_support::*;
use super::supervisor_provider_wait_support::*;

#[cfg(unix)]
fn continuous(persisted: bool, extra: &[&str]) {
    use super::provider_wait_quiet_support::*;
    use std::time::Instant;

    let fixture = fixture();
    if persisted {
        seed_wait(&fixture);
    }
    let expected_starts = usize::from(!persisted);
    let start = Instant::now();
    let mut run = start_text_run(&fixture, "supervisor", extra);
    if !persisted {
        // Establish the reported situation before testing whether the run lives.
        // wait_for allows the observation even if the run has already halted.
        wait_for("a recognized supervisor provider refusal", &mut run, || {
            run_output(&fixture).contains("provider-limited by anthropic")
        });
    }
    assert_held(&fixture, expected_starts);
    eprintln!(
        "before sleep: persisted={persisted}, args={extra:?}, exit={:?}, elapsed={:?}, \
         supervisor=supervising, child=work, spawns={expected_starts}; {}",
        run.child().try_wait().expect("inspect run status"),
        start.elapsed(),
        run.output()
    );
    let watched = watch_the_sleep(&fixture, &mut run, "supervisor", || {
        line_count(&fixture.root.join("starts"))
    });
    watched.assert_quiet("provider-held supervisor with an open child");
    assert_eq!(pass_headers(&watched.stdout), expected_starts, "{}", watched.stdout);
    let status = interrupt(&mut run);
    eprintln!(
        "persisted={persisted}, args={extra:?}, interrupt exit={status}, elapsed={:?}; {}",
        start.elapsed(),
        run.output()
    );
    assert_eq!(status.code(), Some(130), "{}", run.output());
    assert!(start.elapsed() >= QUIET_WINDOW);
    assert!(!run.output().contains("halted with non-terminal tasks remaining"));
    assert_held(&fixture, expected_starts);
}

#[cfg(unix)]
fn run_output(fixture: &ProviderFixture) -> String {
    std::fs::read_to_string(fixture.dir.join("supervisor.stdout")).unwrap_or_default()
}

#[cfg(unix)]
#[test]
fn supervisor_provider_wait_fresh_sequential() {
    continuous(false, &[]);
}

#[cfg(unix)]
#[test]
fn supervisor_provider_wait_fresh_parallel() {
    continuous(false, &["--parallel", "2"]);
}

#[cfg(unix)]
#[test]
fn supervisor_provider_wait_persisted_sequential() {
    continuous(true, &[]);
}

#[cfg(unix)]
#[test]
fn supervisor_provider_wait_persisted_parallel() {
    continuous(true, &["--parallel", "2"]);
}

fn until_idle(extra: &[&str]) {
    let fixture = fixture();
    let deadline = seed_wait(&fixture);
    let mut args = vec!["--until-idle"];
    args.extend_from_slice(extra);
    let mut run = fixture.start(&args);
    let status = run.wait_for_exit("the supervisor's idle result");
    assert_held(&fixture, 0);
    let finished = fixture
        .events()
        .into_iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap_or_else(|| panic!("missing finish event: {}", fixture.output()));
    let stop = &finished["summary"]["stop"];
    // Check the missing deadline first: this is the shared eligibility defect.
    assert_eq!(stop["next_attempt_at"], deadline, "{}", fixture.output());
    assert_eq!(status.code(), Some(3), "{}", fixture.output());
    assert_eq!(stop["reason"], "idle");
}

#[test]
fn supervisor_provider_wait_until_idle_sequential() {
    until_idle(&[]);
}

#[test]
fn supervisor_provider_wait_until_idle_parallel() {
    until_idle(&["--parallel", "2"]);
}

// A held child's own gate cannot replace its holder's provider blocker. §FS-rhei-run-report.3.1
fn gated_child_until_idle(extra: &[&str]) {
    let fixture = fixture();
    std::fs::write(&fixture.machine, MACHINE.replace("  work:\n", "  work:\n    gating: true\n"))
        .unwrap();
    let deadline = seed_wait(&fixture);
    let mut args = vec!["--until-idle"];
    args.extend_from_slice(extra);
    let mut run = fixture.start(&args);
    let status = run.wait_for_exit("the gated child's holder to report its provider wait");
    assert_held(&fixture, 0);
    let finished = fixture
        .events()
        .into_iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap_or_else(|| panic!("missing finish event: {}", fixture.output()));
    let stop = &finished["summary"]["stop"];
    assert_eq!(status.code(), Some(3), "{}", fixture.output());
    assert_eq!(stop["reason"], "idle");
    assert_eq!(stop["idle_blocker"], "provider_limit", "{}", fixture.output());
    assert_eq!(stop["next_attempt_at"], deadline, "{}", fixture.output());
}

#[test]
fn supervisor_provider_wait_gated_child_until_idle_sequential() {
    gated_child_until_idle(&[]);
}

#[test]
fn supervisor_provider_wait_gated_child_until_idle_parallel() {
    gated_child_until_idle(&["--parallel", "2"]);
}

// A provider reset cannot supply an absent required supervisor input. §FS-rhei-run.5.1
fn missing_input(extra: &[&str]) {
    let fixture = fixture();
    std::fs::write(
        &fixture.machine,
        MACHINE.replace(
            "    visits: 6\n",
            "    visits: 6\n    inputs:\n      - name: brief\n        path: runtime/missing-brief.md\n",
        ),
    )
    .unwrap();
    seed_wait(&fixture);
    let mut run = fixture.start(extra);
    let status = run.wait_for_exit("the missing supervisor input to halt the run");
    let output = fixture.output();
    assert_eq!(status.code(), Some(1), "{output}");
    assert!(output.contains("required input(s) not found: brief"), "{output}");
    assert!(output.contains("missing-brief.md"), "{output}");
    assert!(!output.contains("sleeping"), "{output}");
    assert!(!fixture.events().iter().any(|e| e["event"] == "task_spawned"), "{output}");
    assert_held(&fixture, 0);
}

#[test]
fn supervisor_provider_wait_missing_input_sequential() {
    missing_input(&[]);
}

#[test]
fn supervisor_provider_wait_missing_input_parallel() {
    missing_input(&["--parallel", "2"]);
}

#[test]
fn supervisor_provider_wait_missing_input_until_idle_sequential() {
    missing_input(&["--until-idle"]);
}

#[test]
fn supervisor_provider_wait_missing_input_until_idle_parallel() {
    missing_input(&["--until-idle", "--parallel", "2"]);
}
