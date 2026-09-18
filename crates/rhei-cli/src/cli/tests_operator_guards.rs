// Shared forced preflight's preserved task and state safeguards. §FS-rhei-transition-cmd.6

fn operator_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() { walk(root, &path, out); }
            else { out.insert(path.strip_prefix(root).unwrap().to_path_buf(), fs::read(path).unwrap()); }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}

/// Each refusal checks its own diagnostic and every affected file, including metadata. §FS-rhei-transition-cmd.6
#[test]
fn operator_preflight_preserves_the_guard_matrix() {
    for case in ["stale", "unknown", "profile", "result", "outputs", "inputs", "claim", "descendant-claim", "descendants", "visits", "ancestor", "supervisor-claim"] {
        let (dir, plan, machine) = operator_fixture();
        let mut request = operator_request(&plan, &machine);
        let mut body = fs::read_to_string(&plan).unwrap();
        let mut yaml = OPERATOR_MACHINE.to_string();
        let expected = match case {
            "stale" => { request.from = "work"; "conflict:" }
            "unknown" => { request.to = "missing"; "not a valid state" }
            "profile" => { yaml.push_str("profiles:\n  default:\n    initial: gate\n    allowed: [gate, done, cancelled]\nnode_policy:\n  root: default\n  default: default\n"); "not allowed by its resolved profile" }
            "result" => { request.to = "cancelled"; request.result = Some(" \t"); "requires a fresh non-empty --result" }
            "outputs" => { yaml = yaml.replace("    gating: true", "    gating: true\n    outputs:\n      - {name: proof, path: runtime/proof.md}"); "Missing required output artifact" }
            "inputs" => { yaml = yaml.replace("    visits: 3", "    visits: 3\n    inputs:\n      - {name: proof, path: runtime/proof.md}"); "Missing required input artifact" }
            "claim" => { body.push_str("**Assignee:** worker\n"); "assigned to worker" }
            "descendant-claim" => { body.push_str("\n#### Task 1.1: Child\n**State:** gate\n**Assignee:** worker\n"); "assigned to worker" }
            "descendants" => { body.push_str("\n#### Task 1.1: Child\n**State:** gate\n"); request.to = "cancelled"; "descendant tasks remain non-terminal" }
            "visits" => { body = body.replace("# Rhei: Recovery\n", "# Rhei: Recovery\n---\nmetadata:\n  tasks:\n    '1':\n      stateVisits:\n        work: 3\n---\n"); "budget for state 'work' is exhausted" }
            "ancestor" => { body = "# Rhei: Recovery\n\n## Tasks\n\n### Task 1: Parent\n**State:** done\n\n#### Task 1.1: Child\n**State:** done\n".into(); request.task = "1.1"; request.from = "done"; "terminal ancestor plan.1 must be reopened" }
            _ => {
                yaml = yaml.replace("  work:\n", "  supervising:\n    agent: pi\n    execute_on: descendant-terminal\n    visits: 3\n  work:\n");
                yaml.push_str("  - {from: supervising, to: supervising}\n  - {from: supervising, to: done}\n");
                body = "# Rhei: Recovery\n\n## Tasks\n\n### Task 1: Parent\n**State:** supervising\n**Assignee:** supervisor\n\n#### Task 1.1: Child\n**State:** gate\n".into();
                request.task = "1.1";
                "assigned to supervisor"
            }
        };
        fs::write(&plan, body).unwrap(); fs::write(&machine, yaml).unwrap();
        let before = operator_snapshot(dir.path());
        let error = prepare_forced_transition(&request).err().expect(case);
        assert!(error.to_string().contains(expected), "{case}: {error}");
        assert_eq!(operator_snapshot(dir.path()), before, "{case}");
    }
}

/// A declared edge keeps task-owned guards as well as its rule condition. §FS-rhei-transition-cmd.6
#[test]
fn operator_declared_edges_report_the_blocking_task_guard() {
    for case in ["outputs", "inputs", "claim", "result"] {
        let (dir, plan, machine) = operator_fixture();
        let mut yaml = format!("{OPERATOR_MACHINE}  - {{from: gate, to: work}}\n");
        let mut request = operator_request(&plan, &machine);
        let expected = match case {
            "outputs" => { yaml = yaml.replace("    gating: true", "    gating: true\n    outputs:\n      - {name: proof, path: runtime/proof.md}"); "Missing required output artifact" }
            "inputs" => { yaml = yaml.replace("    visits: 3", "    visits: 3\n    inputs:\n      - {name: proof, path: runtime/proof.md}"); "Missing required input artifact" }
            "claim" => { fs::write(&plan, format!("{}**Assignee:** worker\n", fs::read_to_string(&plan).unwrap())).unwrap(); "assigned to worker" }
            _ => { request.to = "done"; request.result = None; "without a result" }
        };
        fs::write(&machine, yaml).unwrap();
        let before = operator_snapshot(dir.path());
        let error = prepare_forced_transition(&request).err().unwrap().to_string();
        assert!(error.contains(expected), "{case}: {error}");
        assert!(error.ends_with("--force does not bypass safeguards on declared edges"), "{error}");
        assert_eq!(operator_snapshot(dir.path()), before);
    }
}

/// Reserved cancellation waives source outputs; Prior is intentionally not a force guard. §FS-rhei-transition-cmd.6
#[test]
fn operator_preflight_preserves_cancellation_waiver_and_prior_omission() {
    let (_dir, plan, machine) = operator_fixture();
    fs::write(&machine, OPERATOR_MACHINE.replace("    gating: true", "    gating: true\n    outputs:\n      - {name: proof, path: runtime/missing.md}")).unwrap();
    let mut request = operator_request(&plan, &machine);
    request.to = "cancelled";
    let prepared = prepare_forced_transition(&request).unwrap();
    assert_eq!(prepared.files.len(), 2);
    fs::write(&machine, OPERATOR_MACHINE).unwrap();
    let body = fs::read_to_string(&plan).unwrap();
    fs::write(&plan, format!("{body}**Prior:** 2\n\n### Task 2: Still open\n**State:** work\n")).unwrap();
    request.to = "work";
    let prepared = prepare_forced_transition(&request).unwrap();
    let task = prepared.files.iter().find(|file| file.roles.iter().any(|r| r == "task")).unwrap();
    let after = String::from_utf8(task.after.bytes().unwrap().unwrap()).unwrap();
    assert!(after.contains("**State:** work"));
    assert!(after.contains("stateVisits:"));
    assert!(after.contains("**Prior:** 2"));
}

/// Directory workspaces include separate task/metadata/checkpoint/result images. §FS-rhei-recover.2
#[test]
fn operator_workspace_images_preserve_checkpoint_and_result_once() {
    for point in ["image-0-before", "image-0-after", "image-1-before", "image-1-after", "image-2-before", "image-2-after", "pair-between", "ledger-sync-after"] {
    let (dir, _, machine) = operator_fixture();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let index = root.join("index.rhei.md");
    fs::create_dir_all(root.join("tasks")).unwrap();
    fs::write(&index, "# Rhei: Recovery\n\n").unwrap();
    fs::write(root.join("tasks/work.md"), "### Task 1: Parent\n**State:** supervising\n\n#### Task 1.1: Fix\n**State:** work\n").unwrap();
    let yaml = OPERATOR_MACHINE.replace("  work:\n", "  supervising:\n    agent: pi\n    execute_on: descendant-terminal\n    visits: 3\n  work:\n")
        + "  - {from: supervising, to: supervising}\n  - {from: supervising, to: done}\n";
    fs::write(&machine, yaml).unwrap();
    let mut request = operator_request(&index, &machine);
    request.task = "1.1"; request.from = "work"; request.to = "done";
    let prepared = prepare_forced_transition(&request).unwrap();
    assert_eq!(prepared.files.len(), 3);
    let metadata = prepared.files.iter().find(|file| file.path == "index.rhei.md").unwrap();
    assert!(metadata.roles.iter().any(|r| r == "checkpoint"));
    let after = String::from_utf8(metadata.after.bytes().unwrap().unwrap()).unwrap();
    assert!(after.contains("checkpoints"), "{after}");
    interrupt_force_at(point);
    assert!(commit_confirmed_force(&request, prepared, "test").is_err());
    clear_force_interrupt();
    let marker = recorded_marker(&root);
    let decision = forced_decision(&root, &marker).unwrap();
    forced_replay(&root, &marker, decision).unwrap();
    for image in marker.files {
        assert_eq!(ForcedImage::read(&root.join(image.path)).unwrap(), if decision == ForcedDecision::Forward { image.after } else { image.before });
    }
}
}

/// A concurrent claim acquired after confirmation is seen under the root guard. §FS-rhei-transition-cmd.6
#[test]
fn operator_locked_revalidation_detects_a_new_claim() {
    let (dir, plan, machine) = operator_fixture();
    let request = operator_request(&plan, &machine);
    let preview = prepare_forced_transition(&request).unwrap();
    let path = plan.clone();
    FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
        if point == "confirmed-before-locks" {
            fs::write(&path, format!("{}**Assignee:** raced-worker\n", fs::read_to_string(&path).unwrap())).unwrap();
        }
        Ok(())
    })));
    let error = commit_confirmed_force(&request, preview, "test").unwrap_err();
    clear_force_interrupt();
    assert!(error.to_string().contains("assigned to raced-worker"), "{error}");
    assert!(!dir.path().join(rhei_core::root_access::MARKER).exists());
}

/// Re-entry keeps counted metadata and the ordinary suffix convention. §FS-rhei-transition-cmd.6
#[test]
fn operator_terminal_reopening_spends_the_next_visit() {
    let (dir, plan, machine) = operator_fixture();
    let mut request = operator_request(&plan, &machine);
    commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").unwrap();
    transition_command(&plan, &[], Some(&machine), "1", "work", "cancelled", Some("cancelled"), None, true).unwrap();
    request.from = "cancelled";
    commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").unwrap();
    let body = fs::read_to_string(&plan).unwrap();
    assert!(body.contains("**State:** work-2"), "{body}");
    assert!(!body.contains("> **Result:**"));
    assert_eq!(fs::read_to_string(dir.path().join("runtime/results/plan.1.md")).unwrap().matches("## Result").count(), 3);
}

/// Ordinary wildcard cancellation cannot leave a final state. §FS-rhei-transitions.4.6
#[test]
fn operator_recovery_is_required_for_a_terminal_wildcard_source() {
    let (dir, plan, machine) = operator_fixture();
    fs::write(&plan, fs::read_to_string(&plan).unwrap().replace("**State:** gate", "**State:** done")).unwrap();
    fs::write(&machine, format!("{OPERATOR_MACHINE}  - {{from: '*', to: cancelled}}\n")).unwrap();
    let before = fs::read(&plan).unwrap();
    let error = transition_command(&plan, &[], Some(&machine), "1", "done", "cancelled", Some("replace outcome"), None, true).unwrap_err();
    assert!(error.to_string().contains("not allowed by the state machine"), "{error}");
    assert_eq!(fs::read(&plan).unwrap(), before);
    assert!(!dir.path().join("runtime/state-transitions.log").exists());
}
