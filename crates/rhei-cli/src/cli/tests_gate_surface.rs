// The live and static surfaces meet a gate through the core `rhei gate show`
// and `rhei gate answer` run: the brief they draw is that document, and an
// answer a surface posts is checked, dry-run and written by the same steps.

// §FS-rhei-viz.5.1.1 §FS-rhei-viz.5.1.2

/// The plan, its machines and the answer core a run's surfaces would hold.
fn surface_scene(plan: &Path, states: &Path) -> (LoadedPlan, ResolvedMachineSet, ExecutionMachines) {
    let loaded = load_plan(plan).expect("load plan");
    let resolved = resolve_state_machines_for_loaded_plan(plan, &loaded, Some(states)).expect("machines");
    let machines = ExecutionMachines::build(&resolved, plan, &loaded).expect("execution machines");
    (loaded, resolved, machines)
}

/// The brief a surface draws is `rhei gate show`'s, field for field, and a
/// ticket that waits at no gate has none. §FS-rhei-viz.5.1.1
#[test]
fn a_surface_draws_the_brief_gate_show_prints() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let (plan, states, _) = briefed_gate_plan(dir.path());
    let (loaded, resolved, _) = surface_scene(&plan, &states);

    let briefs = surface_gate_briefs(&loaded, &plan, &resolved.validator_set());
    assert_eq!(briefs.keys().collect::<Vec<_>>(), ["plan.1"]);
    let brief = &briefs["plan.1"];
    assert_eq!((brief.state.as_str(), brief.source.as_str()), ("review", "questions"));
    assert!(brief.authored(), "a static question is an authored brief");
    let question = &brief.questions[0];
    assert_eq!((question.id.as_str(), question.kind.as_str(), question.required), ("approve", "boolean", true));
    let routes: Vec<_> = brief.routes.iter().map(|route| route.to.as_str()).collect();
    assert_eq!(routes, ["completed", "rework"]);
    assert!(brief.digest.starts_with("sha256:"), "{}", brief.digest);

    let set = resolved.validator_set();
    let task = flatten_tasks(&loaded.rhei).into_iter().find(|task| task.id.to_string() == "plan.1").expect("task");
    let source = GateViewSource { loaded: &loaded, input: &plan, machine: set.for_task(&task.id), task };
    let shown = serde_json::to_value(GateShowDocument::of(&gate_view(&source).unwrap().unwrap())).unwrap();
    assert_eq!(serde_json::to_value(brief).unwrap(), shown, "the surface reads the document `gate show` prints");

    fs::write(&plan, "# Rhei: Gate\n\n## Tasks\n\n### Task 1: Review\n**State:** rework\n\nReview.\n").unwrap();
    let (loaded, resolved, _) = surface_scene(&plan, &states);
    assert!(surface_gate_briefs(&loaded, &plan, &resolved.validator_set()).is_empty(), "no gate, no brief");
}

/// A dry run names the destination and writes nothing; the answer writes the
/// answer file and moves the ticket where the routes say; a refusal is the
/// outcome's, not an error. §FS-rhei-viz.5.1.2 §FS-rhei-gate-cmd.3
#[test]
fn a_surface_answer_takes_the_steps_gate_answer_takes() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let (plan, states, _) = briefed_gate_plan(dir.path());
    let (loaded, resolved, machines) = surface_scene(&plan, &states);
    let digest = surface_gate_briefs(&loaded, &plan, &resolved.validator_set())["plan.1"].digest.clone();
    let core = SurfaceAnswer { input: &plan, machines: &machines, no_callbacks: true };
    let document = |answers: &str| format!(r#"{{"version":1,"request_digest":"{digest}","answers":{answers}}}"#);

    let missing = core.answer("plan.1", "review", &document("{}"), true).expect("an outcome");
    assert!(!missing.ok);
    assert_eq!(missing.errors[0].question.as_deref(), Some("approve"), "{:?}", missing.errors);

    let dry = core.answer("plan.1", "review", &document(r#"{"approve":true}"#), true).expect("an outcome");
    assert!(dry.ok && dry.dry_run, "{:?}", dry.errors);
    assert_eq!(dry.to.as_deref(), Some("completed"));
    let answer_file = dir.path().join("runtime/gates/plan.1/review.1.answer.json");
    assert!(!answer_file.exists(), "a dry run writes nothing");
    assert!(fs::read_to_string(&plan).unwrap().contains("**State:** review"));

    let stale = core.answer("plan.1", "review", r#"{"version":1,"request_digest":"sha256:0","answers":{"approve":true}}"#, false)
        .expect("an outcome");
    assert!(!stale.ok, "an answer to a brief nobody read is refused");
    assert!(!answer_file.exists());

    let done = core.answer("plan.1", "review", &document(r#"{"approve":true}"#), false).expect("an outcome");
    assert!(done.ok && !done.dry_run, "{:?}", done.errors);
    assert_eq!(done.to.as_deref(), Some("completed"));
    assert!(answer_file.exists(), "the answer is kept beside the gate");
    assert!(fs::read_to_string(&plan).unwrap().contains("**State:** completed"));

    let err = core.answer("plan.9", "review", &document("{}"), true).err().expect("an unknown ticket is an error");
    assert!(err.to_string().contains("task 'plan.9' not found"), "{err}");
}
