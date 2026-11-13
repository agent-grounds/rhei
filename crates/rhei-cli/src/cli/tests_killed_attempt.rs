// An attempt killed together with its run, below `rhei run`: each condition of
// the classifier, the order of the refusal's cases, the reading that predicts
// and the spawn site that writes, the charges and the poll bound, and `rhei
// run` finalizing a killed run before it plans a spawn.

// §FS-rhei-evidence.11 §FS-rhei-agents.8.1.1 §FS-rhei-agents.3.2.3

#[cfg(unix)]
mod killed_attempt {
    use super::super::*;
    use std::os::unix::process::CommandExt as _;

    const TASK: &str = "plan.1";
    const STATE: &str = "cover";
    const TRANSCRIPT: &str = "the killed attempt's transcript\n";

    /// The group of a process that has exited and been reaped: gone.
    fn gone_group() -> i32 {
        let mut child =
            std::process::Command::new("true").process_group(0).spawn().expect("spawn true");
        child.wait().expect("reap true");
        child.id() as i32
    }

    /// A group with `sleep 30` still in it, killed when the test ends.
    struct LiveGroup(std::process::Child);

    impl LiveGroup {
        fn start() -> Self {
            let child = std::process::Command::new("sleep")
                .arg("30")
                .process_group(0)
                .spawn()
                .expect("spawn sleep");
            LiveGroup(child)
        }

        fn pgid(&self) -> i32 {
            self.0.id() as i32
        }
    }

    impl Drop for LiveGroup {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// What a killed run left about one spawn of `cover`, each field one of the
    /// conditions of §FS-rhei-evidence.11.1 at the value that satisfies it.
    struct KilledRun {
        /// The manifest's `status`; `None` for a run nobody has finalized.
        status: Option<&'static str>,
        end_record: bool,
        kind: &'static str,
        task: &'static str,
        state: &'static str,
        moves: u64,
        attempt: u64,
        /// The `log_path` its start record names, where not the planned log.
        log_path: Option<&'static str>,
        /// Bytes the log gains after finalization hashed it.
        edited: bool,
        /// The process group its group record names; `None` for no record.
        group: Option<i32>,
    }

    impl Default for KilledRun {
        fn default() -> Self {
            KilledRun {
                status: Some("incomplete"),
                end_record: false,
                kind: "agent",
                task: TASK,
                state: STATE,
                moves: 0,
                attempt: 1,
                log_path: None,
                edited: false,
                group: Some(gone_group()),
            }
        }
    }

    struct Workspace {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Workspace {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tmpdir");
            let root = rhei_core::platform::canonical_path(dir.path()).expect("canonical");
            Workspace { _dir: dir, root }
        }

        fn runtime(&self) -> PathBuf {
            self.root.join("runtime")
        }

        /// The first spawn of `cover` as the records plan it.
        fn plan(&self) -> SpawnPlan {
            plan_spawn_attempt(&self.runtime(), &self.root, TASK, STATE, None, LogNumber::Entry(1))
        }

        /// Leave what `killed` describes, with the planned log as its transcript.
        fn kill(&self, killed: &KilledRun) -> SpawnPlan {
            let plan = self.plan();
            fs::create_dir_all(plan.log.parent().expect("logs")).expect("logs dir");
            fs::write(&plan.log, TRANSCRIPT).expect("write log");
            let log_path = killed
                .log_path
                .map_or_else(|| evidence_relative(&self.root, &plan.log), str::to_string);
            let id = format!(
                "{}::{}::mock::visit-1::run-dead::move-{}::attempt-{}",
                killed.task, killed.state, killed.moves, killed.attempt
            );
            let run_dir = evidence_dir(&self.root).join("runs").join("run-dead");
            let key = evidence_key(&id);
            let write = |name: String, value: serde_json::Value| {
                write_evidence_exclusively(&run_dir.join(name), &value).expect("write evidence")
            };
            write(
                format!("{key}.start.json"),
                serde_json::json!({
                    "kind": killed.kind, "phase": "start", "invocation_id": id,
                    "task_id": killed.task, "state": killed.state, "log_path": log_path,
                    "run_id": "run-dead", "started_at": "2026-11-13T10:00:00Z",
                }),
            );
            if let Some(pgid) = killed.group {
                let group = serde_json::json!({ "invocation_id": id, "process_group": pgid });
                write(format!("{key}.group.json"), group);
            }
            if killed.end_record {
                write(format!("{key}.end.json"), serde_json::json!({ "invocation_id": id }));
            }
            if let Some(status) = killed.status {
                let files = [serde_json::json!({
                    "path": log_path, "sha256": sha256_hex(TRANSCRIPT.as_bytes()),
                })];
                write("manifest.json".into(), serde_json::json!({ "status": status, "files": files }));
            }
            if killed.edited {
                fs::write(&plan.log, format!("{TRANSCRIPT}planted\n")).expect("edit log");
            }
            plan
        }

        fn classify(&self, killed: &KilledRun, program: bool) -> TakenLog {
            let plan = self.kill(killed);
            classify_taken_log(&plan, &self.root, TASK, STATE, program)
        }
    }

    fn unaccounted(verdict: &TakenLog) -> bool {
        matches!(verdict, TakenLog::Unaccounted)
    }

    /// All six hold: the log is a killed attempt, started when its record says.
    // §FS-rhei-evidence.11.1
    #[test]
    fn all_six_conditions_account_for_the_log_as_a_killed_attempt() {
        let verdict = Workspace::new().classify(&KilledRun::default(), false);
        let TakenLog::Killed { started } = verdict else { panic!("{verdict:?}") };
        assert_eq!(started, "2026-11-13T10:00:00Z");
    }

    /// 1: a run nobody finalized is a run that may still be alive, so a reading
    /// before finalization predicts the refusal; a run that ended is no kill.
    // §FS-rhei-evidence.11.1 §FS-rhei-evidence.9.3
    #[test]
    fn condition_1_a_run_not_finalized_as_incomplete_accounts_for_nothing() {
        for status in [None, Some("completed"), Some("halted")] {
            let killed = KilledRun { status, ..KilledRun::default() };
            let verdict = Workspace::new().classify(&killed, false);
            assert!(unaccounted(&verdict), "{status:?}: {verdict:?}");
        }
    }

    /// 2: a process with an end record finished; its run's death ended nothing.
    #[test]
    fn condition_2_a_start_with_an_end_record_accounts_for_nothing() {
        let killed = KilledRun { end_record: true, ..KilledRun::default() };
        assert!(unaccounted(&Workspace::new().classify(&killed, false)));
    }

    /// 3: a program fails only the `kind`, and is refused as one; an agent's
    /// record at a program state is no account at all.
    // §FS-rhei-evidence.11.1 §FS-rhei-agents.8.1.1
    #[test]
    fn condition_3_a_program_is_its_own_case() {
        let program = KilledRun { kind: "program", ..KilledRun::default() };
        let verdict = Workspace::new().classify(&program, true);
        assert!(matches!(verdict, TakenLog::Program), "{verdict:?}");
        assert!(unaccounted(&Workspace::new().classify(&KilledRun::default(), true)));
    }

    /// 4: another task, state, move or attempt, or another log, is not this spawn.
    #[test]
    fn condition_4_another_spawn_accounts_for_nothing() {
        let cases = [
            KilledRun { task: "plan.2", ..KilledRun::default() },
            KilledRun { state: "review", ..KilledRun::default() },
            KilledRun { moves: 1, ..KilledRun::default() },
            KilledRun { attempt: 2, ..KilledRun::default() },
            KilledRun { log_path: Some("runtime/logs/other.log"), ..KilledRun::default() },
        ];
        for killed in &cases {
            let verdict = Workspace::new().classify(killed, false);
            assert!(unaccounted(&verdict), "{verdict:?}");
        }
    }

    /// 5: a log whose bytes changed after finalization is no longer the attempt's.
    #[test]
    fn condition_5_an_edited_log_accounts_for_nothing() {
        let killed = KilledRun { edited: true, ..KilledRun::default() };
        assert!(unaccounted(&Workspace::new().classify(&killed, false)));
    }

    /// 6: with no group record nothing says the agent stopped; with its group
    /// still running, the refusal lists what is left in it.
    // §FS-rhei-evidence.11.1 §FS-rhei-evidence.11.2
    #[test]
    fn condition_6_the_group_record_must_exist_and_its_group_be_gone() {
        let killed = KilledRun { group: None, ..KilledRun::default() };
        let verdict = Workspace::new().classify(&killed, false);
        assert!(matches!(verdict, TakenLog::NoGroupRecord), "{verdict:?}");

        let live = LiveGroup::start();
        let killed = KilledRun { group: Some(live.pgid()), ..KilledRun::default() };
        let TakenLog::StillRunning(processes) = Workspace::new().classify(&killed, false) else {
            panic!("a live group is still running");
        };
        assert!(
            processes.iter().any(|p| p.pid == live.0.id() && p.command.contains("sleep 30")),
            "{processes:?}"
        );
    }

    /// A live group wins over every other case: over a program, and over a log
    /// that no longer matches its hash. §FS-rhei-agents.8.1.1
    #[test]
    fn a_live_group_beats_a_program_and_a_hash_mismatch() {
        let live = LiveGroup::start();
        let group = Some(live.pgid());
        let program = KilledRun { kind: "program", group, ..KilledRun::default() };
        let verdict = Workspace::new().classify(&program, true);
        assert!(matches!(verdict, TakenLog::StillRunning(_)), "{verdict:?}");
        let edited = KilledRun { edited: true, group, ..KilledRun::default() };
        let verdict = Workspace::new().classify(&edited, false);
        assert!(matches!(verdict, TakenLog::StillRunning(_)), "{verdict:?}");
    }

    /// Each case's refusal: a live group lists its processes and offers no
    /// `kill`, move or reset; a program lists its transitions; a missing group
    /// record says why; otherwise the refusal is today's, word for word.
    // §FS-rhei-agents.8.1.1 §FS-rhei-agents.8.1
    #[test]
    fn each_case_is_refused_in_its_own_words() {
        let log = Path::new("/w/runtime/logs/task-plan.1-cover.log");
        let reset = ResetOffer::new(Path::new("/w"));
        let processes = vec![rhei_core::platform::worker_liveness::LiveProcess {
            pid: 4242,
            command: "sleep 300".into(),
        }];
        let running = TakenLog::StillRunning(processes).refusal(log, TASK, Some(&reset), &[]);
        assert!(running.contains("its process group still has processes"), "{running}");
        assert!(running.contains("pid 4242: sleep 300"), "{running}");
        for offered in ["kill -", "rhei reset", "move that file"] {
            assert!(!running.contains(offered), "{offered}: {running}");
        }

        let edge = "  rhei transition /w --task plan.1 --from=finalize --to=done".to_string();
        let program = TakenLog::Program.refusal(log, TASK, Some(&reset), &[edge.clone()]);
        assert!(program.contains("it may have finished"), "{program}");
        assert!(program.contains(&edge) && program.contains("rhei reset"), "{program}");

        let today = unaccounted_log_message(log, TASK, Some(&reset));
        let no_group = TakenLog::NoGroupRecord.refusal(log, TASK, Some(&reset), &[]);
        assert!(no_group.contains("exists and no spawn record accounts for it"), "{no_group}");
        assert!(no_group.contains("rhei cannot tell whether that agent has stopped"));
        assert_eq!(TakenLog::Unaccounted.refusal(log, TASK, Some(&reset), &[]), today);
    }

    fn agent_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            "name: killed\nversion: 1\nstates:\n  cover:\n    initial: true\n    agent: mock\n  \
             done:\n    final: true\ntransitions:\n  - from: cover\n    to: done\n",
        )
        .expect("machine")
    }

    /// A reading predicts the killed attempt's record and plans past it, and
    /// writes nothing: no spawn record, the log untouched. §FS-rhei-evidence.11.1
    #[test]
    fn a_reading_predicts_the_resume_without_writing() {
        let ws = Workspace::new();
        let killed = ws.kill(&KilledRun::default());
        let reset = ResetOffer::new(&ws.root);
        let (plan, _) =
            plan_named_spawn(&agent_machine(), &ws.root, &ws.runtime(), TASK, STATE, 1, None, &reset)
                .expect("a killed attempt is planned past");
        assert_eq!(plan.attempt, 2);
        assert_eq!(plan.charged, 0, "the first kill is free");
        let previous = plan.previous.as_ref().expect("the predicted record");
        assert_eq!((previous.ending.as_str(), previous.killed), (KILLED_ENDING, 1));
        assert!(!killed.record.exists(), "a reading writes no spawn record");
        assert_eq!(fs::read_to_string(&killed.log).expect("log"), TRANSCRIPT);
    }

    /// The spawn site writes the killed attempt's record before it plans the
    /// next one, and a second kill of the visit is charged.
    // §FS-rhei-agents.8.1.1 §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.4
    #[test]
    fn the_spawn_site_records_the_kill_and_charges_the_second() {
        let ws = Workspace::new();
        let killed = ws.kill(&KilledRun::default());
        fs::write(ws.root.join("plan.rhei.md"), "# Rhei: Plan\n\n## Tasks\n\n### Task 1: Cover\n**State:** cover\n")
            .expect("plan");
        let loaded = load_plan(&ws.root.join("plan.rhei.md")).expect("load");
        let task = find_task_by_id(&loaded.rhei.tasks, &parse_task_id(TASK)).expect("task");
        let machine = agent_machine();
        let settings = RheiSettings::default();
        let reset = ResetOffer::new(&ws.root);
        let site = SpawnSite { machine: &machine, task, loaded: &loaded, settings: &settings, reset: &reset };
        let (plan, _) =
            site.plan(&ws.root, &ws.runtime(), STATE, 1, None, "mock").expect("planned");
        assert_eq!(plan.attempt, 2);
        let record = read_spawn_record(&killed.record).expect("the killed attempt's record");
        assert_eq!((record.ending.as_str(), record.killed, record.attempt), (KILLED_ENDING, 1, 1));
        assert!(!record.attempt_charged && record.code.is_none() && record.log == killed.log);
        assert_eq!(record.worker, "mock");

        let second = killed_record(&plan, None, "2026-11-13T11:00:00Z", TASK, STATE, "mock");
        assert_eq!((second.killed, second.attempt, second.charged), (2, 2, 1));
        assert!(second.attempt_charged, "every kill after the first is charged");
    }

    /// In a poll state kills are the bound: past one plus what grants added,
    /// the visit is spent; any other state is bounded through its charges.
    // §FS-rhei-agents.3.2.3 §FS-rhei-retry.2
    #[test]
    fn a_poll_visit_is_spent_past_one_kill_plus_its_grants() {
        let ws = Workspace::new();
        let mut plan = ws.plan();
        let poll = AttemptBudget::Poll { max_attempts: 5 };
        plan.previous = Some(killed_record(&plan, None, "s", TASK, STATE, "mock"));
        assert_eq!(plan.kills_spent(poll), None, "the first kill is free");
        let mut twice = killed_record(&plan, plan.previous.as_ref(), "s", TASK, STATE, "mock");
        plan.previous = Some(twice.clone());
        assert_eq!(plan.kills_spent(poll), Some(2));
        assert_eq!(plan.kills_spent(AttemptBudget::Visit(2)), None);
        twice.grants.push(AttemptGrant { at: "t".into(), os_user: "op".into(), attempts: 2 });
        plan.previous = Some(twice);
        assert_eq!(plan.kills_spent(poll), None, "a grant of 2 allows two more kills");
        let halt = kill_halt_line(TASK, "waiting", 2, 2);
        assert!(halt.contains("killed together with its run 2 times on this visit"), "{halt}");
        assert!(halt.contains("rhei retry plan.1"), "{halt}");
    }

    /// `rhei run` finalizes the killed run before it plans its first spawn, so
    /// a log a reading still refuses is resumed by the run itself.
    // §FS-rhei-evidence.11.1 §FS-rhei-evidence.9.3 §FS-rhei-run.3.2
    #[test]
    fn rhei_run_finalizes_the_killed_run_before_it_plans_a_spawn() {
        let ws = Workspace::new();
        let plan_path = ws.root.join("plan.rhei.md");
        fs::write(&plan_path, "# Rhei: Plan\n\n## Tasks\n\n### Task 1: Cover\n**State:** cover\n")
            .expect("plan");
        let states = ws.root.join("states.yaml");
        fs::write(
            &states,
            "name: killed\nversion: 1\nstates:\n  cover:\n    initial: true\n    agent: mock\n    \
             attempts: 1\n  done:\n    final: true\ntransitions:\n  - from: cover\n    to: done\n",
        )
        .expect("states");
        let settings = ws.root.join(".agent-grounds/rhei");
        fs::create_dir_all(&settings).expect("settings dir");
        fs::write(
            settings.join("settings.json"),
            r#"{ "agents": { "mock": { "command": ["sh", "-c", "cat >/dev/null"],
                 "stdin_prompt": true, "intervene_stdin": false, "timeout": "60s" } } }"#,
        )
        .expect("settings");
        let killed = ws.kill(&KilledRun { status: None, ..KilledRun::default() });
        let reset = ResetOffer::new(&plan_path);
        let reading = plan_named_spawn(
            &agent_machine(), &ws.root, &ws.runtime(), TASK, STATE, 1, None, &reset,
        );
        assert!(reading.is_err(), "unfinalized, the reading predicts the refusal");

        let mut opts = default_run_options();
        opts.standalone.continue_on_error = true;
        opts.standalone.no_tui = true;
        let _ = run_command(&plan_path, Some(&states), opts);
        let resumed = ws.runtime().join("logs").join(format!("task-{TASK}-{STATE}-attempt2.log"));
        assert!(resumed.exists(), "the run resumed the visit as its next attempt");
        assert_eq!(fs::read_to_string(&killed.log).expect("log"), TRANSCRIPT, "kept as it was");
        let record = read_spawn_record(&killed.record).expect("record");
        assert!(record.attempt >= 2 && record.killed == 1, "the kill carried forward: {record:?}");
    }
}

/// On Windows the worker is its own process, gone only when its pid names no
/// process or one created at another time. §FS-rhei-evidence.11.2
#[cfg(windows)]
mod killed_attempt_windows {
    use super::super::*;
    use rhei_core::platform::worker_liveness::{process_created_at, WorkerLiveness};

    #[test]
    fn a_group_record_names_a_process_by_its_pid_and_creation_time() {
        let pid = std::process::id();
        let created_at = process_created_at(pid).expect("own creation time");
        let record = |created_at: u64| serde_json::json!({ "pid": pid, "created_at": created_at });
        assert!(matches!(
            recorded_worker_liveness(&record(created_at)),
            Some(WorkerLiveness::Running(_))
        ));
        assert_eq!(recorded_worker_liveness(&record(created_at + 1)), Some(WorkerLiveness::Gone));
        let unrecorded = serde_json::json!({ "pid": pid });
        assert_eq!(recorded_worker_liveness(&unrecorded), None, "no creation time, no record");
    }
}
