// The run-input seal's record, change kinds and sealed set, and the settings
// copy a strict run reads once. §FS-rhei-seal

fn seal_fixture() -> (tempfile::TempDir, SealScope) {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("scripts/nested")).unwrap();
    fs::write(dir.path().join("states.yaml"), "name: m\n").unwrap();
    fs::write(dir.path().join("scripts/step.py"), "print('laid')\n").unwrap();
    fs::write(dir.path().join("scripts/nested/helper.py"), "x = 1\n").unwrap();
    let machine = dir.path().join("states.yaml");
    let scope = SealScope::new("drift", dir.path(), Some(&machine), dir.path(), None);
    (dir, scope)
}

fn file(sha: &str, executable: Option<bool>) -> SealEntry {
    SealEntry::File { sha256: sha.to_string(), executable }
}

fn snapshot_of(entries: &[(&str, SealEntry)]) -> SealSnapshot {
    SealSnapshot {
        machine: Some("states.yaml".to_string()),
        directories: BTreeMap::from([("scripts".to_string(), true)]),
        entries: entries.iter().map(|(path, entry)| (path.to_string(), entry.clone())).collect(),
    }
}

fn kinds(changes: &[SealChange]) -> Vec<(&str, &str)> {
    changes.iter().map(|change| (change.path.as_str(), change.kind)).collect()
}

#[test]
fn a_generation_is_canonical_json_that_reads_back_to_what_it_sealed() {
    // §FS-rhei-seal.3.4 §FS-rhei-seal.3.2
    let (_dir, scope) = seal_fixture();
    let snapshot = take_seal_snapshot(&scope).unwrap();
    let text = seal_record_text(&scope, &snapshot, 1, SealTaker::Run("run-1"));
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(canonical_json_text(&value), text, "keys sorted, no insignificant whitespace");
    assert_eq!(parse_seal_record(&value), Some(snapshot.clone()));
    assert_eq!(value["taken_by"], "run");
    assert_eq!(value["run_id"], "run-1");
    let paths: Vec<&str> =
        value["entries"].as_array().unwrap().iter().map(|e| e["path"].as_str().unwrap()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "entries sorted by path");
    for relative in ["states.yaml", "scripts/step.py", "scripts/nested/helper.py"] {
        assert!(paths.contains(&relative), "{relative} relative to the base, with `/`: {paths:?}");
    }
    let verb = seal_record_text(&scope, &snapshot, 2, SealTaker::Verb);
    let verb: serde_json::Value = serde_json::from_str(&verb).unwrap();
    assert_eq!((verb["taken_by"].as_str(), verb["run_id"].is_null()), (Some("rhei seal"), true));
}

#[test]
fn each_difference_has_one_kind() {
    // §FS-rhei-seal.4
    let sealed = snapshot_of(&[
        ("a.py", file("1", Some(false))),
        ("b.py", file("2", Some(false))),
        ("c.py", file("3", Some(false))),
        ("d", SealEntry::Symlink { target: "a.py".into(), sha256: Some("1".into()) }),
        ("e.json", SealEntry::Absent),
        ("f.py", file("6", None)),
    ]);
    let current = snapshot_of(&[
        ("a.py", file("9", Some(false))),
        ("b.py", file("2", Some(true))),
        ("d", SealEntry::Symlink { target: "b.py".into(), sha256: Some("2".into()) }),
        ("e.json", file("5", Some(false))),
        ("f.py", file("6", Some(true))),
        ("g.py", file("7", Some(false))),
    ]);
    assert_eq!(
        kinds(&seal_changes(&sealed, &current)),
        [
            ("a.py", "modified"),
            ("b.py", "mode"),
            ("c.py", "deleted"),
            ("d", "link"),
            ("e.json", "added"),
            ("g.py", "added"),
        ],
        "an unknown executable bit is never compared: f.py is unchanged"
    );
    let as_file = snapshot_of(&[("d", file("1", Some(false)))]);
    let as_link = snapshot_of(&[("d", SealEntry::Symlink { target: "x".into(), sha256: None })]);
    assert_eq!(kinds(&seal_changes(&as_link, &as_file)), [("d", "link")]);
    assert!(seal_changes(&sealed, &sealed).is_empty());
}

#[test]
fn a_directory_or_a_machine_that_appears_or_goes_is_a_change() {
    // §FS-rhei-seal.4 §FS-rhei-seal.3.2
    let mut sealed = snapshot_of(&[]);
    sealed.directories.insert("prompt_templates".into(), false);
    let mut current = sealed.clone();
    current.directories.insert("prompt_templates".into(), true);
    current.directories.insert("scripts".into(), false);
    assert_eq!(
        kinds(&seal_changes(&sealed, &current)),
        [("prompt_templates", "added"), ("scripts", "deleted")]
    );
    let built_in = SealSnapshot::default();
    let own = SealSnapshot { machine: Some("states.yaml".into()), ..SealSnapshot::default() };
    assert_eq!(kinds(&seal_changes(&built_in, &own)), [("states.yaml", "added")]);
    assert_eq!(kinds(&seal_changes(&own, &built_in)), [("states.yaml", "deleted")]);
}

#[test]
fn an_edit_on_disk_is_found_against_the_latest_generation() {
    // §FS-rhei-seal.3.1 §FS-rhei-seal.8.2
    let (dir, scope) = seal_fixture();
    assert!(latest_seal_generation(&scope).unwrap().is_none());
    let first = take_seal(&scope, SealTaker::Verb).unwrap();
    assert_eq!((first.generation, first.previous.is_none()), (1, true));
    let bytes = fs::read(scope.seal_dir().join("1.json")).unwrap();
    fs::write(dir.path().join("scripts/step.py"), "print('edited')\n").unwrap();
    let (generation, sealed) = latest_seal_generation(&scope).unwrap().unwrap();
    let changes = seal_changes(&sealed, &take_seal_snapshot(&scope).unwrap());
    assert_eq!((generation, kinds(&changes)), (1, vec![("scripts/step.py", "modified")]));
    let second = take_seal(&scope, SealTaker::Verb).unwrap();
    assert_eq!(second.generation, 2);
    assert_eq!(fs::read(scope.seal_dir().join("1.json")).unwrap(), bytes, "written once");
    assert_eq!(sealed_line("drift", &second), "Sealed rhei `drift`: generation 2 (generation 1 kept)");
    assert!(write_seal_generation(&scope, 2, "{}").is_err(), "an existing generation is never replaced");
}

#[cfg(unix)]
#[test]
fn an_unreadable_sealed_file_is_an_error_naming_it_never_a_change() {
    // §FS-rhei-seal.4: a change's path is a sealed path; strict_check and admission share recheck.
    use std::os::unix::fs::PermissionsExt;
    let (dir, scope) = seal_fixture();
    let sealed = take_seal_snapshot(&scope).unwrap();
    let mut entry = StrictRhei { scope, generation: 1, sealed, drift: None };
    let script = dir.path().join("scripts/step.py");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&script).is_ok() {
        return; // Root reads it anyway; there is nothing unreadable to test.
    }
    let err = entry.recheck().expect_err("an unreadable sealed file is an error");
    assert!(err.to_string().contains("scripts/step.py"), "names the file: {err}");
    assert!(entry.drift.is_none(), "no change is cached for a file never read");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!entry.recheck().unwrap(), "readable again and unchanged: no drift");
}

#[cfg(unix)]
#[test]
fn a_symlink_is_recorded_with_its_text_and_a_directory_link_is_not_followed() {
    // §FS-rhei-seal.3.3
    let (dir, scope) = seal_fixture();
    std::os::unix::fs::symlink("step.py", dir.path().join("scripts/alias.py")).unwrap();
    std::os::unix::fs::symlink("nested", dir.path().join("scripts/dir-link")).unwrap();
    let snapshot = take_seal_snapshot(&scope).unwrap();
    let step = sha256_hex(&fs::read(dir.path().join("scripts/step.py")).unwrap());
    assert_eq!(
        snapshot.entries["scripts/alias.py"],
        SealEntry::Symlink { target: "step.py".into(), sha256: Some(step) }
    );
    assert_eq!(
        snapshot.entries["scripts/dir-link"],
        SealEntry::Symlink { target: "nested".into(), sha256: None }
    );
    assert!(!snapshot.entries.contains_key("scripts/dir-link/helper.py"));
    assert!(matches!(snapshot.entries["states.yaml"], SealEntry::File { executable: Some(false), .. }));
}

#[test]
fn the_sealed_set_names_both_project_candidates_and_the_price_book() {
    // §FS-rhei-seal.2.2 §FS-rhei-seal.2.3
    let dir = tempfile::tempdir().expect("tempdir");
    let book = tempfile::tempdir().expect("tempdir");
    let prices = book.path().join("prices.json");
    fs::write(&prices, "{}").unwrap();
    fs::create_dir_all(dir.path().join(".agent-grounds/rhei")).unwrap();
    fs::write(dir.path().join(".agent-grounds/rhei/settings.json"), "{}").unwrap();
    fs::create_dir_all(dir.path().join("scripts")).unwrap();
    fs::write(dir.path().join("scripts/unsealed.py"), "").unwrap();
    let scope = SealScope::new("plan", dir.path(), None, dir.path(), Some(&prices));
    let snapshot = take_seal_snapshot(&scope).unwrap();
    assert_eq!((snapshot.machine.as_ref(), snapshot.directories.len()), (None, 0));
    assert!(matches!(snapshot.entries[".agent-grounds/rhei/settings.json"], SealEntry::File { .. }));
    assert_eq!(snapshot.entries[".agents/rhei/settings.json"], SealEntry::Absent);
    let label = std::path::absolute(&prices).unwrap().display().to_string();
    assert!(matches!(snapshot.entries[&label], SealEntry::File { .. }), "outside the base: absolute");
    assert!(!snapshot.entries.keys().any(|path| path.contains("scripts/")));
    fs::write(&prices, "{\"edited\":true}").unwrap();
    let changes = seal_changes(&snapshot, &take_seal_snapshot(&scope).unwrap());
    assert_eq!(kinds(&changes), [(label.as_str(), "modified")], "the price book is sealed");
}

#[test]
fn a_strict_run_reads_settings_once_and_every_later_read_gets_that_copy() {
    // §FS-rhei-seal.8.3
    let dir = tempfile::tempdir().expect("tempdir");
    let kept = RheiSettings::default();
    let guard = keep_strict_settings(&kept);
    fs::create_dir_all(dir.path().join(".agent-grounds/rhei")).unwrap();
    fs::write(dir.path().join(".agent-grounds/rhei/settings.json"), "not json").unwrap();
    let read = load_merged_settings(dir.path()).expect("the kept copy, not the file on disk");
    assert!(read.agents.is_empty(), "nothing re-read the settings file");
    drop(guard);
    assert!(load_merged_settings(dir.path()).is_err(), "disarmed, the run reads as it always has");
}

#[test]
fn a_detached_run_keeps_strict_inputs() {
    // §FS-rhei-run.3.8 §FS-rhei-run-headless.1
    let given = ["run", "plan.rhei.md", "--headless", "--strict-inputs"];
    let child = child_arguments_from(given.iter().map(std::ffi::OsString::from));
    assert_eq!(child, ["run", "plan.rhei.md", "--strict-inputs"].map(std::ffi::OsString::from));
}
