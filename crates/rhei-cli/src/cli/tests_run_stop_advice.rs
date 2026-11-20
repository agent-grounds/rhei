// Shared registry fixtures keep this proof inside the registry test group.
// §FS-rhei-errors.1.2 §FS-rhei-transition-cmd.3.4

fn issue_757_registry_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(issue_757_registry_snapshot(&path));
        } else {
            let bytes = if path.ends_with(".rhei/run.lock") {
                read_run_lock_owner(&path).unwrap().into_bytes()
            } else { fs::read(&path).unwrap() };
            files.insert(path, bytes);
        }
    }
    files
}

/// A marker in an unrelated registry entry blocks stop before path resolution.
/// Advice must use that same rule without creating guards or pruning evidence.
/// §FS-rhei-errors.1.2 §FS-rhei-transition-cmd.3.4
#[test]
fn issue_757_stop_advice_observes_registry_first_resolution_without_mutation() {
    for blocked in [false, true] {
        let _registry = IsolatedRegistry::new();
        let target = workspace();
        let other = workspace();
        let superseded = descriptor("old111", &target.path, "2026-11-20T00:00:00Z");
        publish_run_descriptor(&superseded);
        let run = descriptor("live22", &target.path, "2026-11-20T01:00:00Z");
        publish_run_descriptor(&run);
        let _held = held_run_lock_for(&run);
        publish_run_descriptor(&descriptor("other1", &other.path, "2026-11-20T00:00:00Z"));
        if blocked {
            fs::create_dir_all(other.path.join(".rhei")).unwrap();
            fs::write(other.path.join(rhei_core::root_access::MARKER), "{}").unwrap();
        }
        fs::write(run_registry_dir().unwrap().join("malformed.json"), "{malformed").unwrap();
        let state = PathBuf::from(std::env::var_os("XDG_STATE_HOME").unwrap());
        let before = [issue_757_registry_snapshot(&target.path), issue_757_registry_snapshot(&other.path),
            issue_757_registry_snapshot(&state)];
        let verified = verified_operator_run(&target.path).expect("exact live owner and descriptor");
        let resolution = resolve_run_with_access(target.path.to_str(), RunReadAccess::Observe);
        let advice = live_run_stop_advice(&target.path, Some(&verified), true);
        if blocked {
            assert!(resolution.unwrap_err().to_string().contains("forced recovery pending"));
            assert!(!advice.contains("rhei stop "), "{advice}");
            assert!(advice.contains("original launcher") && advice.contains("Ctrl-C"), "{advice}");
        } else {
            let resolved = resolution.unwrap();
            assert_eq!((resolved.id, resolved.pid), (run.id.clone(), run.pid));
            assert_eq!(run_root_key(&resolved.workspace), run_root_key(&target.path));
            if cfg!(unix) {
                assert!(advice.contains(&format!("rhei stop {} --wait", shell_word(&target.path))), "{advice}");
            } else {
                assert!(!advice.contains("rhei stop ") && advice.contains("original launcher"), "{advice}");
            }
            let mut wrong = verified.clone();
            wrong.id = "different-run".into();
            assert!(!live_run_stop_advice(&target.path, Some(&wrong), true).contains("rhei stop "));
        }
        assert_eq!([issue_757_registry_snapshot(&target.path), issue_757_registry_snapshot(&other.path),
            issue_757_registry_snapshot(&state)], before);
    }
}
