// What `remove_runtime_path` records. The reset event and the partly-reset line read it,
// and a removal that fails is the one case an end-to-end run cannot reach past the check.
// §FS-rhei-reset.4 §FS-rhei-evidence.10.1

/// A walk over links in the tree names each link as one entry and stops: it does not
/// go round a loop, and it names nothing outside. §FS-rhei-reset.2
#[cfg(unix)]
#[test]
fn files_under_takes_a_symlink_as_one_entry() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("tempdir");
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).expect("a tree outside");
    fs::write(outside.join("kept.txt"), "kept\n").expect("a file outside");
    let runtime = dir.path().join("runtime");
    fs::create_dir_all(runtime.join("d")).expect("the tree");
    fs::write(runtime.join("d/file"), "x\n").expect("a file");
    symlink("..", runtime.join("up")).expect("a link up");
    symlink(".", runtime.join("d/here")).expect("a link to itself");
    symlink(&outside, runtime.join("out")).expect("a link out");
    symlink("/nonexistent/rhei-609", runtime.join("dangling")).expect("a dangling link");

    let mut files = files_under(&runtime);
    files.sort();

    let expected = ["dangling", "d/file", "d/here", "out", "up"].map(|name| runtime.join(name));
    let mut expected = expected.to_vec();
    expected.sort();
    assert_eq!(files, expected);
}

/// A link is still there after a failed removal, wherever it points, so it has not
/// landed: nothing is recorded, and the error stays free of the partly-reset line.
#[cfg(unix)]
#[test]
fn a_link_that_survives_a_failed_removal_is_not_recorded_as_removed() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let dir = tempfile::tempdir().expect("tempdir");
    let runtime = dir.path().join("runtime");
    let held = runtime.join("held");
    fs::create_dir_all(&held).expect("the tree");
    symlink("/nonexistent/rhei-609", held.join("dangling")).expect("a dangling link");
    symlink(dir.path(), held.join("live")).expect("a live link");
    fs::set_permissions(&held, fs::Permissions::from_mode(0o555)).expect("chmod a-w");
    if fs::File::create(held.join(".chmod-probe")).is_ok() {
        fs::set_permissions(&held, fs::Permissions::from_mode(0o755)).expect("chmod back");
        eprintln!("skipped: chmod a-w does not bind on {}", held.display());
        return;
    }

    let mut progress = ResetProgress::default();
    let outcome = remove_runtime_path(&runtime, &mut progress.removed_paths);

    fs::set_permissions(&held, fs::Permissions::from_mode(0o755)).expect("chmod back");
    let err = outcome.expect_err("a read-only directory stops the removal");
    assert!(fs::symlink_metadata(held.join("dangling")).is_ok(), "fixture: the link survived");
    assert_eq!(progress.removed_paths, Vec::<PathBuf>::new(), "nothing went");
    let stopped = progress.stopped(err).to_string();
    assert!(!stopped.contains(RESET_STOPPED_PART_WAY), "nothing landed:\n{stopped}");
}
