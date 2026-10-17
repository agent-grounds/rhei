//! Registry evidence alone reserves ids at startup and live admission, including
//! Windows' canonical verbatim spelling. §FS-rhei-run.2.7

use super::*;

fn registry_only_frozen_candidate(parallel: usize) {
    let (dir, input) = fixture(true);
    let root = dir.path();
    missing_agent(&run_at(&input, root, FIRST, parallel));
    let occupied = current_id(root);
    // SystemTime retains this instant at the native platform's precision.
    assert_eq!(occupied, short_run_id(std::time::UNIX_EPOCH + Duration::from_nanos(FIRST)));
    let registry = root.join(".state/rhei/runs").join(format!("{occupied}.json"));
    let mut retained = read_descriptor(&registry).expect("retained external descriptor");
    // Put the only reservation at a participating member. On Windows the raw
    // canonical key has the prefix that lock-root normalization strips.
    retained.workspace = fs::canonicalize(root.join("alpha")).unwrap();
    if cfg!(windows) {
        assert_ne!(retained.workspace, run_root_key(&retained.workspace));
    }
    write_descriptor(&registry, &retained).unwrap();
    let retained_bytes = fs::read(&registry).unwrap();
    for workspace in [root.to_path_buf(), root.join("alpha"), root.join("beta")] {
        let runtime = workspace.join("runtime");
        if runtime.exists() {
            fs::remove_dir_all(&runtime).unwrap();
        }
        assert!(!runtime.exists(), "only the external descriptor may reserve the id");
    }

    // Exercise the common reader through the real live-admission validator,
    // isolated from other tests' registry environment and run/budget state.
    assert_eq!(
        run_fixture_at(
            &input,
            root,
            FIRST,
            parallel,
            "run_id_allocation_tests::registry::registry_admission_fixture_child",
        ),
        "registry admission checked"
    );
    assert_eq!(fs::read(&registry).unwrap(), retained_bytes);

    missing_agent(&run_at(&input, root, FIRST, parallel));
    let selected = current_id(root);
    assert_ne!(selected, occupied, "registry-only history must reserve the frozen candidate");
    identity_consistent(root, &selected);
    for artifact in [".rhei/run.lock", "runtime/events.jsonl", "runtime/run-report.md"] {
        assert!(fs::read_to_string(root.join(artifact)).unwrap().contains(&selected), "{artifact}");
    }
    let selected_registry = registry.with_file_name(format!("{selected}.json"));
    assert_eq!(read_descriptor(&selected_registry).unwrap().id, selected);
    assert_eq!(fs::read(&registry).unwrap(), retained_bytes, "retain occupied registry evidence");
}

#[test]
fn run_id_499_sequential_registry_only_frozen_candidate() {
    registry_only_frozen_candidate(1);
}

#[test]
fn run_id_499_parallel_registry_only_frozen_candidate() {
    registry_only_frozen_candidate(2);
}

// Subprocess entry only; the parent retains precisely one external descriptor.
#[test]
fn registry_admission_fixture_child() {
    let Some(root) = std::env::var_os("RHEI_499_TEST_ROOT") else { return };
    let root = PathBuf::from(root);
    RUN_IDENTITY_TEST_WALL
        .with(|wall| wall.set(Some(std::time::UNIX_EPOCH + Duration::from_nanos(FIRST))));
    let occupied = RunIdentity::new();
    let registry = root.join(".state/rhei/runs").join(format!("{}.json", occupied.id));
    let descriptor = read_descriptor(&registry).expect("registry-only evidence");
    let roots = BTreeSet::from([run_root_key(&root.join("alpha"))]);
    assert_eq!(run_root_key(&descriptor.workspace), *roots.first().unwrap());
    assert!(!root.join("alpha/runtime").exists());

    assert_eq!(occupied.id, descriptor.id);
    let error = validate_admitted_run_identity(&roots, &occupied).unwrap_err().to_string();
    assert!(
        error.contains(&format!(
            "new execution root already reserves this run id '{}'",
            occupied.id
        )),
        "{error}"
    );
    let raw_roots = BTreeSet::from([fs::canonicalize(root.join("alpha")).unwrap()]);
    assert!(RunIdentityHistory::read_run_evidence(&raw_roots).unwrap().ids.contains(&occupied.id));
    let selected = RunIdentity::allocate(&roots).unwrap();
    RUN_IDENTITY_TEST_WALL.with(|wall| wall.set(None));
    assert_ne!(selected.id, occupied.id, "the shared reader must reserve registry history");
    validate_admitted_run_identity(&roots, &selected).unwrap();
    // An unrelated registry workspace must not reserve ids in a different root.
    let unrelated = BTreeSet::from([run_root_key(&root.join("beta"))]);
    validate_admitted_run_identity(&unrelated, &occupied).unwrap();
    assert!(!root.join("alpha/runtime").exists(), "inspection must not publish artifacts");
    fs::write(root.join("fixture-result.txt"), "registry admission checked").unwrap();
}
