// The machine-wide registry: what a sweep keeps, what it prunes, and how a
// reference resolves.
// §FS-rhei-run-headless.2 §FS-rhei-run-headless.3 §FS-rhei-run-headless.6

mod run_registry_tests {
    use super::run_descriptor_tests::{
        descriptor, held_run_lock_for, publish_ended, workspace, IsolatedRegistry,
    };
    use super::super::*;

    include!("tests_run_stop_advice.rs");

    fn ids(runs: &[RunDescriptor]) -> Vec<&str> {
        runs.iter().map(|run| run.id.as_str()).collect()
    }

    #[test]
    fn live_runs_are_listed_newest_first() {
        let _registry = IsolatedRegistry::new();
        let older = workspace();
        let newer = workspace();
        let older_run = descriptor("old111", &older.path, "2026-08-22T10:00:00Z");
        let newer_run = descriptor("new222", &newer.path, "2026-08-22T18:00:00Z");
        publish_run_descriptor(&older_run);
        publish_run_descriptor(&newer_run);
        let _held_older = held_run_lock_for(&older_run);
        let _held_newer = held_run_lock_for(&newer_run);

        assert_eq!(ids(&sweep_run_registry().live), vec!["new222", "old111"]);
    }

    /// A lock belongs to its opened inode, not the pathname used to open it.
    /// Replacing that pathname must not make a demonstrably live recorded run
    /// disappear from the registry sweep that feeds both `rhei runs` formats.
    // §FS-rhei-run-headless.3
    #[cfg(target_os = "linux")]
    #[test]
    fn a_live_run_stays_listed_when_its_held_lock_inode_is_replaced() {
        use std::os::unix::fs::MetadataExt;

        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let running = descriptor("inode1", &workspace.path, "2026-09-01T14:14:12Z");
        publish_run_descriptor(&running);

        let mut held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");
        write_run_lock_owner(&mut held, &running.id, running.pid).expect("record lock owner");
        let lock_path = workspace.path.join(".rhei/run.lock");
        let held_path = workspace.path.join(".rhei/run.lock.held-by-test");
        fs::rename(&lock_path, &held_path).expect("rename the held lock inode");
        let held_inode = fs::metadata(&held_path).expect("held lock metadata").ino();
        fs::write(&lock_path, []).expect("install an unlocked replacement");
        let replacement_inode = fs::metadata(&lock_path).expect("replacement metadata").ino();
        assert_ne!(held_inode, replacement_inode, "the pathname must name a new inode");

        let sweep = sweep_run_registry();
        assert_eq!(ids(&sweep.live), vec!["inode1"]);
        assert!(sweep.ended.is_empty());
        assert!(sweep.undecided.is_empty());
    }

    /// The entry is what makes the id resolvable after the fact. Deleting it
    /// broke `rhei attach <id>` at exactly the moment the answer existed.
    // §FS-rhei-run-headless.2 §FS-rhei-run-headless.5.3
    #[test]
    fn an_ended_run_keeps_its_entry_and_stays_out_of_the_live_list() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        publish_ended("gone01", &workspace.path, "2026-08-22T14:03:22Z");

        let sweep = sweep_run_registry();
        assert!(sweep.live.is_empty(), "an ended run is not live");
        assert_eq!(ids(&sweep.ended), vec!["gone01"]);
        assert!(run_registry_path("gone01").expect("path").is_file(), "the entry stays");
        assert_eq!(resolve_run(Some("gone01")).expect("resolves").id, "gone01");
    }

    /// The only verdict that prunes: the workspace no longer names the run.
    #[test]
    fn a_superseded_entry_is_pruned_as_the_list_is_read() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        publish_run_descriptor(&descriptor("ghost1", &workspace.path, "2026-08-22T10:00:00Z"));
        let successor = descriptor("live22", &workspace.path, "2026-08-22T11:00:00Z");
        publish_run_descriptor(&successor);
        let _held = held_run_lock_for(&successor);

        assert_eq!(ids(&sweep_run_registry().live), vec!["live22"]);
        assert!(!run_registry_path("ghost1").expect("path").exists(), "superseded entries go");
    }

    #[test]
    fn a_deleted_workspace_prunes_its_entry() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        publish_run_descriptor(&descriptor("rmrf02", &workspace.path, "2026-08-22T10:00:00Z"));
        fs::remove_dir_all(&workspace.path).expect("delete the workspace");

        assert!(sweep_run_registry().live.is_empty());
        assert!(!run_registry_path("rmrf02").expect("path").exists());
    }

    /// An older binary reading a newer one's registry must not destroy it.
    // §FS-rhei-run-headless.3
    #[test]
    fn an_unparseable_entry_is_kept_and_reported() {
        let _registry = IsolatedRegistry::new();
        let dir = run_registry_dir().expect("registry dir");
        fs::create_dir_all(&dir).expect("registry dir");
        let entry = dir.join("future.json");
        fs::write(&entry, "{\"schema\": \"from a newer rhei\"}\n").expect("entry");

        let sweep = sweep_run_registry();
        assert!(entry.is_file(), "an entry this build cannot read is not an entry to delete");
        assert_eq!(sweep.undecided.len(), 1);
        assert!(sweep.undecided[0].reason.contains("could not be read"));
        assert!(sweep.undecided[0].summary_line().contains("future.json"));
    }

    /// Completion calls the sweep. A transient `EACCES` must not unregister a
    /// live run. §FS-rhei-run-headless.3
    #[cfg(unix)]
    #[test]
    fn an_unreadable_workspace_keeps_its_entry_and_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let running = descriptor("chmod0", &workspace.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&running);
        let _held = held_run_lock_for(&running);
        let rhei_dir = workspace.path.join(".rhei");
        fs::set_permissions(&rhei_dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");

        let sweep = sweep_run_registry();
        fs::set_permissions(&rhei_dir, fs::Permissions::from_mode(0o755)).expect("chmod 755");

        assert!(sweep.live.is_empty(), "it could not be confirmed live");
        assert_eq!(sweep.undecided.len(), 1, "but it is listed, with a reason");
        assert!(run_registry_path("chmod0").expect("path").is_file(), "and it is kept");
        assert_eq!(ids(&sweep_run_registry().live), vec!["chmod0"], "readable again, live again");
    }

    /// Ended entries accumulate. If a prefix search saw them beside live runs,
    /// a two-character prefix that resolves today would start reporting
    /// "matches 4 runs" tomorrow. §FS-rhei-run-headless.3
    #[test]
    fn a_live_run_wins_a_prefix_that_ended_runs_also_match() {
        let _registry = IsolatedRegistry::new();
        let live = workspace();
        let first = workspace();
        let second = workspace();
        let third = workspace();
        publish_ended("ab0001", &first.path, "2026-08-22T10:00:00Z");
        publish_ended("ab0002", &second.path, "2026-08-22T10:30:00Z");
        publish_ended("ab0003", &third.path, "2026-08-22T11:00:00Z");
        let running = descriptor("ab9999", &live.path, "2026-08-22T12:00:00Z");
        publish_run_descriptor(&running);
        let _held = held_run_lock_for(&running);

        assert_eq!(resolve_run(Some("ab")).expect("the live run wins").id, "ab9999");
        // An exact id still reaches an ended run, which is the point of keeping
        // the entry at all.
        assert_eq!(resolve_run(Some("ab0002")).expect("exact").id, "ab0002");
    }

    #[test]
    fn an_exact_id_resolves() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let running = descriptor("abc123", &workspace.path, "2026-08-22T14:03:22Z");
        publish_run_descriptor(&running);
        let _held = held_run_lock_for(&running);
        assert_eq!(resolve_run(Some("abc123")).expect("resolved").id, "abc123");
    }

    #[test]
    fn a_unique_prefix_resolves_and_an_ambiguous_one_lists_the_candidates() {
        let _registry = IsolatedRegistry::new();
        let first = workspace();
        let second = workspace();
        let first_run = descriptor("ab0001", &first.path, "2026-08-22T10:00:00Z");
        let second_run = descriptor("ab0002", &second.path, "2026-08-22T11:00:00Z");
        publish_run_descriptor(&first_run);
        publish_run_descriptor(&second_run);
        let _held_first = held_run_lock_for(&first_run);
        let _held_second = held_run_lock_for(&second_run);

        assert_eq!(resolve_run(Some("ab0001")).expect("exact").id, "ab0001");
        assert!(resolve_run(Some("ab00012")).is_err(), "a longer non-id must not match");

        let err = resolve_run(Some("ab")).expect_err("ambiguous");
        let message = err.to_string();
        assert!(message.contains("matches 2 runs"), "got: {message}");
        assert!(message.contains("ab0001") && message.contains("ab0002"));
    }

    #[test]
    fn an_ambiguous_prefix_among_ended_runs_lists_them_too() {
        let _registry = IsolatedRegistry::new();
        let first = workspace();
        let second = workspace();
        publish_ended("cd0001", &first.path, "2026-08-22T10:00:00Z");
        publish_ended("cd0002", &second.path, "2026-08-22T11:00:00Z");

        let err = resolve_run(Some("cd")).expect_err("ambiguous");
        assert!(err.to_string().contains("matches 2 runs that have ended"), "got: {err}");
    }

    /// The message covers ended runs too, because they resolve too: pointing
    /// only at the live listing points away from half the answers.
    // §FS-rhei-run-headless.3
    #[test]
    fn an_unknown_reference_points_at_the_run_list() {
        let _registry = IsolatedRegistry::new();
        let err = resolve_run(Some("nosuch")).expect_err("unknown");
        assert!(err.to_string().contains("no run matches 'nosuch'"));
        let help = err.help().expect("help").to_string();
        assert!(help.contains("rhei runs"), "got: {help}");
        assert!(help.contains("ended"), "an ended run resolves as well: {help}");
    }

    #[test]
    fn a_path_resolves_to_that_workspaces_descriptor() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        publish_run_descriptor(&descriptor("path01", &workspace.path, "2026-08-22T14:03:22Z"));
        let resolved = resolve_run(Some(&workspace.path.display().to_string())).expect("by path");
        assert_eq!(resolved.id, "path01");
    }

    #[test]
    fn a_workspace_with_no_recorded_run_says_how_to_start_one() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let err = resolve_run(Some(&workspace.path.display().to_string()))
            .expect_err("no run recorded");
        assert!(err.to_string().contains("no run has been recorded"));
        assert!(err.help().expect("help").to_string().contains("--headless"));
    }

    /// Retention is not accumulation: a machine that runs a plan a minute must
    /// not grow an unbounded registry. §FS-rhei-run-headless.2
    #[test]
    fn the_registry_keeps_only_the_newest_ended_runs() {
        let _registry = IsolatedRegistry::new();
        let root = workspace();
        let total = RETAINED_ENDED_RUNS + 5;
        for index in 0..total {
            let each = root.path.join(format!("ws{index:03}"));
            fs::create_dir_all(&each).expect("workspace");
            // Minute-by-minute start times, so "newest first" is well defined.
            publish_ended(
                &format!("e{index:05}"),
                &each,
                &format!("2026-08-22T{:02}:{:02}:00Z", index / 60, index % 60),
            );
        }

        let sweep = sweep_run_registry();
        assert_eq!(sweep.ended.len(), RETAINED_ENDED_RUNS);
        let newest = format!("e{:05}", total - 1);
        let oldest = "e00000";
        assert_eq!(sweep.ended[0].id, newest, "the newest survives");
        assert!(!run_registry_path(oldest).expect("path").exists(), "the oldest is dropped");
        assert!(run_registry_path(&newest).expect("path").is_file());
    }
    /// The tier that resolves first is *not known to have ended*, not *live*.
    /// An entry nobody could check is exactly the one an operator needs
    /// `attach` and `stop` to reach, and it stopped resolving at all the moment
    /// a lock file became unreadable.
    // §FS-rhei-run-headless.3
    #[cfg(unix)]
    #[test]
    fn an_undecided_entry_resolves_by_exact_id_and_by_prefix() {
        use std::os::unix::fs::PermissionsExt;
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        publish_run_descriptor(&descriptor("uk0001", &workspace.path, "2026-08-22T10:00:00Z"));
        let _held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");
        let lock = workspace.path.join(".rhei").join("run.lock");
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o000)).expect("chmod 000");

        let sweep = sweep_run_registry();
        assert!(sweep.live.is_empty(), "the case under test is an entry nobody could check");
        assert_eq!(sweep.undecided.len(), 1);

        let exact = resolve_run(Some("uk0001"));
        let prefix = resolve_run(Some("uk"));
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).expect("chmod 644");

        assert_eq!(exact.expect("an undecided entry resolves by its id").id, "uk0001");
        assert_eq!(prefix.expect("and by a unique prefix").id, "uk0001");
    }

    /// Live first, then undecided: a decided answer wins a tie against one that
    /// could not be checked. §FS-rhei-run-headless.3
    #[cfg(unix)]
    #[test]
    fn a_live_run_is_offered_before_an_undecided_one() {
        use std::os::unix::fs::PermissionsExt;
        let _registry = IsolatedRegistry::new();
        let live = workspace();
        let blind = workspace();
        let live_run = descriptor("lv0001", &live.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&live_run);
        publish_run_descriptor(&descriptor("uk0002", &blind.path, "2026-08-22T11:00:00Z"));
        let _held_live = held_run_lock_for(&live_run);
        let _held_blind = try_acquire_run_lock(&blind.path).expect("lock").expect("available");
        let lock = blind.path.join(".rhei").join("run.lock");
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o000)).expect("chmod 000");

        let sweep = sweep_run_registry();
        let order = sweep.not_known_to_have_ended();
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).expect("chmod 644");

        assert_eq!(
            order.iter().map(|run| run.id.as_str()).collect::<Vec<_>>(),
            vec!["lv0001", "uk0002"],
            "the decided answer comes first"
        );
    }

    /// A tab keypress must not unlink a file. Completion classifies exactly as
    /// the listing does and removes nothing; the listing, which the operator
    /// actually asked for, still prunes.
    // §FS-rhei-run-headless.3
    #[test]
    fn reading_the_registry_for_completion_prunes_nothing() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let gone = descriptor("gone09", &workspace.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&gone);
        // Superseded: the workspace names a later run, so this entry is
        // provably prunable — the one verdict that deletes.
        publish_run_descriptor(&descriptor("next09", &workspace.path, "2026-08-22T11:00:00Z"));
        let entry = run_registry_path("gone09").expect("registry path");
        assert!(entry.is_file(), "the entry must exist for the test to mean anything");

        assert!(
            !ids(&read_run_registry().live).contains(&"gone09"),
            "the superseded entry is not live"
        );
        assert!(entry.is_file(), "a read must not delete what a listing would");

        sweep_run_registry();
        assert!(!entry.exists(), "and the listing still prunes it");
    }

    /// A hundred retained entries share prefixes freely. Printing every match
    /// produced 203 lines of wrapped diagnostic, which is not an answer to
    /// "which one did you mean?".
    // §FS-rhei-run-headless.3
    #[test]
    fn an_ambiguous_prefix_lists_ten_candidates_and_counts_the_rest() {
        let _registry = IsolatedRegistry::new();
        let root = workspace();
        let total = LISTED_AMBIGUOUS_MATCHES + 5;
        for index in 0..total {
            let each = root.path.join(format!("ws{index:03}"));
            fs::create_dir_all(&each).expect("workspace");
            publish_ended(
                &format!("ef{index:04}"),
                &each,
                &format!("2026-08-22T10:{index:02}:00Z"),
            );
        }

        let err = resolve_run(Some("ef")).expect_err("ambiguous");
        let message = err.to_string();
        assert!(message.contains(&format!("matches {total} runs that have ended")), "{message}");
        let listed = message.lines().filter(|line| line.contains("finished")).count();
        assert_eq!(listed, LISTED_AMBIGUOUS_MATCHES, "capped at ten:\n{message}");
        assert!(message.contains("... and 5 more"), "and says how many it left out: {message}");
    }

    /// What `rhei attach <TAB>` offers, in the order resolution tries it: live
    /// first, then the entries nobody could check — which are exactly the runs
    /// an operator needs to reach when something is wrong — then the most
    /// recent ended ones, which `rhei attach` still answers for.
    // §FS-rhei-run-headless.3 §FS-rhei-run-headless.2
    #[cfg(unix)]
    #[test]
    fn completion_offers_live_then_unchecked_then_recently_ended_runs() {
        use std::os::unix::fs::PermissionsExt;
        let _registry = IsolatedRegistry::new();
        let live = workspace();
        let blind = workspace();
        let over = workspace();
        publish_ended("rr0003", &over.path, "2026-08-22T09:00:00Z");
        let live_run = descriptor("rr0001", &live.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&live_run);
        publish_run_descriptor(&descriptor("rr0002", &blind.path, "2026-08-22T11:00:00Z"));
        let _held_live = held_run_lock_for(&live_run);
        let _held_blind = try_acquire_run_lock(&blind.path).expect("lock").expect("available");
        let lock = blind.path.join(".rhei").join("run.lock");
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o000)).expect("chmod 000");

        let offered = complete_run_reference(std::ffi::OsStr::new("rr"));
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).expect("chmod 644");

        let ids: Vec<String> = offered
            .iter()
            .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
            .collect();
        assert_eq!(ids, vec!["rr0001", "rr0002", "rr0003"]);
    }

}
