// What the launcher hands the operator once the handshake has named the child:
// an id that already resolves, and a warning only when it cannot.
// §FS-rhei-run-headless.1.1 §FS-rhei-run-headless.2

mod headless_launch_registry_tests {
    use super::run_descriptor_tests::{descriptor, held_run_lock_for, workspace, IsolatedRegistry};
    use super::super::*;

    const UNREGISTERED: &str = "has no registry entry";

    /// Records, at the first byte the launcher prints, whether the id it is
    /// about to print already has a registry entry — the moment an operator or
    /// a CI step can act on it.
    struct WatchedStdout {
        entry: Option<PathBuf>,
        registered_when_printed: Option<bool>,
        text: Vec<u8>,
    }

    impl WatchedStdout {
        fn new(id: &str) -> Self {
            Self { entry: run_registry_path(id), registered_when_printed: None, text: Vec::new() }
        }
    }

    impl Write for WatchedStdout {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.registered_when_printed.is_none() {
                let registered = self.entry.as_deref().is_some_and(Path::is_file);
                self.registered_when_printed = Some(registered);
            }
            self.text.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The state the launcher sees in the window the ticket hit: the child has
    /// published `runtime/run.json` and holds its run lock, but its registry
    /// entry is not there yet.
    fn running_before_its_entry(id: &str, workspace: &Path) -> (RunDescriptor, HeldRunLock) {
        let running = descriptor(id, workspace, "2026-10-04T02:41:00Z");
        write_descriptor(&run_descriptor_path(workspace), &running).expect("workspace descriptor");
        let held = held_run_lock_for(&running);
        let entry = run_registry_path(id).expect("an isolated state home");
        assert!(!entry.exists(), "the registry entry must be absent for the test to mean anything");
        (running, held)
    }

    /// agent-grounds/rhei#436: `rhei attach --json <id>` right after the
    /// launcher printed `<id>` answered `no run matches`, because the child
    /// writes its registry entry after the workspace descriptor the handshake
    /// reads.
    // §FS-rhei-run-headless.1.1
    #[test]
    fn the_printed_id_resolves_before_the_childs_own_entry_lands() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let (running, _held) = running_before_its_entry("a436f1", &workspace.path);

        let mut out = WatchedStdout::new("a436f1");
        let mut err = Vec::new();
        announce_launch(&LaunchOutcome::Running(running), false, false, &mut out, &mut err)
            .expect("announce the launch");

        let printed = String::from_utf8_lossy(&out.text).into_owned();
        assert!(printed.contains("Run a436f1 started headless"), "printed:\n{printed}");
        assert_eq!(
            out.registered_when_printed,
            Some(true),
            "the id was printed before its registry entry existed"
        );
        let resolved = resolve_run(Some("a436f1"))
            .unwrap_or_else(|error| panic!("the printed id does not resolve: {error:?}"));
        assert_eq!(resolved.id, "a436f1");
        let warned = String::from_utf8_lossy(&err).into_owned();
        assert!(!warned.contains(UNREGISTERED), "a false registry warning:\n{warned}");
    }

    /// The same promise for a run that ended inside the handshake window: its
    /// id is printed too, and `rhei attach <id>` is how CI reads the result.
    // §FS-rhei-run-headless.1.1 §FS-rhei-run-headless.5.3
    #[test]
    fn a_run_that_finished_early_resolves_by_the_id_printed_for_it() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let mut finished = descriptor("a436f2", &workspace.path, "2026-10-04T02:41:00Z");
        finished.status = RunStatus::Finished;
        finished.exit_code = Some(0);
        write_descriptor(&run_descriptor_path(&workspace.path), &finished).expect("workspace descriptor");

        let mut out = WatchedStdout::new("a436f2");
        let mut err = Vec::new();
        announce_launch(&LaunchOutcome::FinishedEarly(finished), true, false, &mut out, &mut err)
            .expect("announce the launch");

        assert_eq!(out.registered_when_printed, Some(true), "the id was printed before it resolved");
        let resolved = resolve_run(Some("a436f2"))
            .unwrap_or_else(|error| panic!("the printed id does not resolve: {error:?}"));
        assert_eq!(resolved.exit_code, Some(0));
        let warned = String::from_utf8_lossy(&err).into_owned();
        assert!(!warned.contains(UNREGISTERED), "a false registry warning:\n{warned}");
    }

    /// A state home the registry cannot be created under — here a regular
    /// file where its directory would go. The warning is the only thing
    /// telling the operator why the id will not resolve, so it must still
    /// fire, say why, and come without a wait.
    // §FS-rhei-run-headless.1.1 §FS-rhei-run-headless.2
    #[cfg(unix)]
    #[test]
    fn an_unwritable_registry_is_warned_about_with_its_reason_and_without_a_wait() {
        let _registry = IsolatedRegistry::new();
        let state = run_registry_dir().expect("an isolated state home");
        let blocker = state.parent().expect("the rhei state directory");
        fs::create_dir_all(blocker.parent().expect("the state home")).expect("state home");
        fs::write(blocker, "not a directory").expect("block the registry directory");
        let workspace = workspace();
        let running = descriptor("a436f3", &workspace.path, "2026-10-04T02:41:00Z");
        write_descriptor(&run_descriptor_path(&workspace.path), &running).expect("workspace descriptor");
        let _held = held_run_lock_for(&running);

        let mut out = Vec::new();
        let mut err = Vec::new();
        let started = Instant::now();
        announce_launch(&LaunchOutcome::Running(running), false, false, &mut out, &mut err)
            .expect("announce the launch");
        let took = started.elapsed();

        let warned = String::from_utf8_lossy(&err).into_owned();
        assert!(warned.contains(UNREGISTERED), "no warning for an id that cannot resolve:\n{warned}");
        assert!(
            warned.contains("Not a directory"),
            "the warning does not say why the entry could not be written:\n{warned}"
        );
        assert!(took < Duration::from_secs(1), "the launcher waited {took:?} on an unwritable registry");
        assert!(String::from_utf8_lossy(&out).contains("Run a436f3 started headless"));
    }

    /// The child may win the race, and its terminal rewrite may already have
    /// landed: an entry that is there is the child's, and stays exactly as it
    /// wrote it.
    // §FS-rhei-run-headless.1.1 §FS-rhei-run-headless.2
    #[test]
    fn an_entry_already_in_the_registry_is_left_as_the_child_wrote_it() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let running = descriptor("a436f4", &workspace.path, "2026-10-04T02:41:00Z");
        write_descriptor(&run_descriptor_path(&workspace.path), &running).expect("workspace descriptor");
        let mut ended = running.clone();
        ended.status = RunStatus::Finished;
        ended.exit_code = Some(3);
        let entry = run_registry_path("a436f4").expect("an isolated state home");
        write_descriptor(&entry, &ended).expect("the child's own entry");
        let before = fs::read(&entry).expect("read the entry");

        let mut out = Vec::new();
        let mut err = Vec::new();
        announce_launch(&LaunchOutcome::Running(running), false, false, &mut out, &mut err)
            .expect("announce the launch");

        assert_eq!(fs::read(&entry).expect("read the entry"), before, "the child's entry was replaced");
        let warned = String::from_utf8_lossy(&err).into_owned();
        assert!(!warned.contains(UNREGISTERED), "a false registry warning:\n{warned}");
    }
}
