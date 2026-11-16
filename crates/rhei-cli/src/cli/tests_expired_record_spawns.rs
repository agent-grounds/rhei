// Whether a run-out provider record is still a deadline for a ticket whose
// visit has spawned since: its spawn record's start, against when each record
// on the identity was observed.
//
// Its own part because the question is the spawn record's, not the parser's or
// the poll's, and the deadline tests beside it are at their size budget.

// §AR-source-file-size.3 §FS-rhei-run.5.1.1

mod expired_record_spawns {
    use super::provider_limits::{codex_openai, deadline};
    use super::*;

    /// Agent work with attempts to spare, so no spent budget answers first.
    const MACHINE: &str = r#"name: expired-record-spawns
version: 1
states:
  working:
    initial: true
    target: codex:openai:gpt-5.6-sol
    attempts: 6
  completed:
    final: true
transitions:
  - from: working
    to: completed
"#;

    /// One ticket in `working` carrying a codex/openai record observed
    /// `observed_ago` seconds ago that runs out `runs_out_in` seconds from now
    /// (negative: already out), and, when `spawned_ago` is set, the visit's last
    /// spawn of the same agent that many seconds ago. Answers the deadline scan,
    /// the instant it read, and the instant the record runs out.
    fn scan(observed_ago: u64, runs_out_in: i64, spawned_ago: Option<u64>) -> [Option<u64>; 3] {
        let mut rhei =
            rhei_core::parse("# Rhei: Limits\n\n## Tasks\n\n### Task 1: Works\n**State:** working\n")
                .expect("plan");
        let now = current_unix_secs();
        let limit = ProviderLimit {
            identity: ProviderIdentity { agent: "codex".into(), provider: "openai".into() },
            kind: ProviderWaitKind::Limit,
            signal: "signal".into(),
            observed_at: deadline(now - observed_ago),
            next_attempt_at: deadline(now.saturating_add_signed(runs_out_in)),
        };
        let (metadata, _) =
            set_provider_limit_metadata(None, &parse_task_id("1"), "working", &limit);
        rhei.metadata = Some(metadata);
        let machine = rhei_validator::StateMachine::from_yaml_str(MACHINE).expect("machine");
        let machines = rhei_validator::MachineSet::single(machine.clone());
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };
        let dir = tempfile::tempdir().expect("tmpdir");
        let (root, runtime) = (dir.path(), dir.path().join("runtime"));
        let spawns = SpawnSiteRoots { settings: Some(&settings), ..SpawnSiteRoots::plan_only(root, &runtime) };
        if let Some(ago) = spawned_ago {
            let agent = codex_openai();
            let task = &rhei.tasks[0];
            let ticket = (&machine, task, "working");
            record_spawn_at(&spawns, &rhei, ticket, &agent, now - ago);
            let started = last_spawn_started(&spawns, &rhei, ticket, &agent);
            assert_eq!(started, i64::try_from(now - ago).ok(), "the spawn reads back as the visit's last");
        }
        let found = agent_deadline_since(&rhei, &machines, &settings, &None, &spawns, spawns.pending_since());
        [found, Some(spawns.read_at), Some(now.saturating_add_signed(runs_out_in))]
    }

    /// Write `agent`'s spawn of the ticket's current visit, started at `started`,
    /// where its spawn site plans it.
    fn record_spawn_at(
        spawns: &SpawnSiteRoots<'_>,
        rhei: &rhei_core::ast::Rhei,
        (machine, task, state): (&rhei_validator::StateMachine, &rhei_core::ast::Task, &str),
        agent: &ResolvedAgent,
        started: u64,
    ) {
        let task_id = task.id.to_string();
        let root = spawns.tasks.artifact_root(&task_id);
        let visits = render_visit_count(rhei.metadata.as_ref(), &task.id, state, state, machine);
        let runtime = spawns.machines.agent_runtime_dir(&task_id, root, spawns.runtime_dir);
        let identity = resolved_agent_log_identity(agent);
        let reset = &spawns.machines.reset;
        let (plan, _) =
            plan_named_spawn(machine, root, &runtime, &task_id, state, visits, identity.as_deref(), reset)
                .expect("plan the spawn");
        plan.record_spawn(SpawnEnding {
            task_id: &task_id,
            state_name: state,
            kind: "agent",
            worker: "codex",
            started: &deadline(started),
            ended: &deadline(started + 1),
            duration: "1s",
            code: Some(1),
            ending: "exited",
        });
    }

    /// The issue's record: observed and run out before the visit last spawned the
    /// ticket. That spawn ran past it, so it is no deadline and releases no stall.
    /// §FS-rhei-run.5.1.1 §FS-rhei-run.3.6
    #[test]
    fn a_run_out_record_the_visit_spawned_past_is_no_deadline() {
        let [found, ..] = scan(7200, -3600, Some(60));
        assert_eq!(found, None, "the stalled ticket must halt, not wake for a spent record");
    }

    /// A record observed after the visit's last spawn started is that spawn's own,
    /// written as it parked or backed off; once it runs out the ticket is eligible
    /// now, so a deadline that moved under the run costs one rescan.
    /// §FS-rhei-run.3.3 §FS-rhei-run.5.1
    #[test]
    fn a_run_out_record_observed_after_the_spawn_started_still_means_now() {
        let [found, read_at, _] = scan(60, -1, Some(120));
        assert_eq!(found, read_at, "the spawn's own record still wakes the run");
        let [found, read_at, _] = scan(60, -1, Some(60));
        assert_eq!(found, read_at, "observed in the second the spawn started is its own");
    }

    /// With no spawn in the visit there is nothing the record could have been met
    /// by: the clamp's "now" stands, as it does for a refill the deadline outran.
    /// §FS-rhei-run.3.3
    #[test]
    fn a_run_out_record_on_an_unspawned_visit_still_means_now() {
        let [found, read_at, _] = scan(7200, -3600, None);
        assert_eq!(found, read_at);
    }

    /// Only a record that has run out can be spent: a live one is the ticket's
    /// wait whenever the visit last spawned it.
    /// §FS-rhei-run.3.3
    #[test]
    fn a_live_record_is_the_deadline_whatever_the_visit_spawned() {
        let [found, _, runs_out] = scan(7200, 600, Some(60));
        assert_eq!(found, runs_out);
    }
}
