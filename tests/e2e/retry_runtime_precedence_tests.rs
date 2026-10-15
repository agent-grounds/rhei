// Recovery follows the next run's active runtime, not the newest duplicate.
// §FS-rhei-retry.2 §FS-rhei-retry.3
use super::supervision_retry_support::text;
use super::*;

const FOLLOW_MACHINE: &str = r#"name: retry-runtime-follow
version: 1
states:
  work:
    initial: true
    concurrent: true
    description: Work
    attempts: 1
    target: mock:openai:model-a
    agent_timeout: 10s
  done:
    description: Done
    final: true
transitions:
  - {from: work, to: done}
"#;

struct Fixture {
    _temp: TestDir,
    project: PathBuf,
    hidden: PathBuf,
    home: PathBuf,
    parallel: bool,
}

impl Fixture {
    fn new(parallel: bool) -> Self {
        let temp = unique_temp_dir("retry-runtime-precedence");
        let project = temp.join("project");
        let hidden = temp.join("hidden/follow");
        fs::create_dir_all(project.join("seed/tasks")).unwrap();
        fs::create_dir_all(hidden.join("tasks")).unwrap();
        write_fixture_file(&project, "index.panta.md", "# Panta: Retry runtime\n");
        write_fixture_file(&hidden, "index.rhei.md", "# Rhei: Follow\n");
        write_fixture_file(&hidden, "states.yaml", FOLLOW_MACHINE);
        write_fixture_file(&hidden, "tasks/01-work.md", "### Task 1: Work\n**State:** work\n");
        let worker = write_python_agent(
            &project,
            "worker.py",
            r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'attempts.log', env('RHEI_VISIT_COUNT') + ':' + env('RHEI_ATTEMPT') + '\n')
if not (root / 'recover').exists():
    sys.exit(1)
result('Recovered the active project invocation\n')
"#,
        );
        super::agent_reentry_support::write_settings(&project, &worker, false);
        super::agent_reentry_support::write_settings(&hidden, &worker, false);
        let publisher = write_python_agent(
            &project,
            "publish.py",
            &format!(
                "pathlib.Path({}).rename(pathlib.Path({}))\nresult('Published follow\\n')\n",
                serde_json::to_string(&hidden).unwrap(),
                serde_json::to_string(&project.join("follow")).unwrap()
            ),
        );
        let seed = project.join("seed");
        write_fixture_file(&seed, "index.rhei.md", "# Rhei: Seed\n");
        write_fixture_file(
            &seed,
            "states.yaml",
            &format!(
                r#"name: retry-runtime-seed
version: 1
states:
  publish:
    initial: true
    concurrent: true
    description: Publish
    program:
      command: {}
    program_timeout: 10s
  finished:
    description: Finished
    final: true
transitions:
  - {{from: publish, to: finished, exit_code: 0}}
"#,
                fixture_command(&publisher)
            ),
        );
        write_fixture_file(
            &seed,
            "tasks/01-publish.md",
            "### Task 1: Publish\n**State:** publish\n",
        );
        let home = temp.join("home");
        Self { _temp: temp, project, hidden, home, parallel }
    }

    fn command(&self, cwd: &Path, args: &[&str]) -> CliRun {
        let mut command = rhei_command(&self.home);
        // Only isolated fixture processes discard the enclosing worker context.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("RHEI_") {
                command.env_remove(key);
            }
        }
        CliRun::from(&command.current_dir(cwd).args(args).output().unwrap())
    }

    fn run(&self) -> CliRun {
        self.command(
            &self.project,
            &[
                "run",
                ".",
                "--no-tui",
                "--no-callbacks",
                "--continue-on-error",
                "--parallel",
                if self.parallel { "2" } else { "1" },
            ],
        )
    }

    fn retry(&self) -> CliRun {
        self.command(&self.project, &["retry", "follow.1"])
    }

    fn failed_run(&self) {
        let out = self.run();
        assert!(!out.status.success(), "{}", text(&out));
    }
}

fn record(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn assert_recovery(parallel: bool) {
    let fixture = Fixture::new(parallel);
    let member = fixture.project.join("follow");
    let stem = "runtime/spawns/task-follow.1-work-mock-openai-model-a.json";
    let active_path = fixture.project.join(stem);
    let dormant_path = member.join(stem);
    // Both records and their retry history come from real runs; only the dormant
    // ended timestamp is advanced below to make its strict ordering deterministic.
    fixture.failed_run();
    assert!(dormant_path.exists());
    assert!(!active_path.exists());
    fixture.failed_run();
    assert_eq!(record(&active_path)["charged"], 1);
    fs::rename(&member, &fixture.hidden).unwrap();
    assert_success(
        &fixture
            .command(fixture.hidden.parent().unwrap(), &["retry", "follow", "--task", "follow.1"]),
    );
    write_fixture_file(
        &fixture.project.join("seed"),
        "tasks/02-publish.md",
        "### Task 2: Re-publish\n**State:** publish\n",
    );
    fixture.failed_run();
    // Keep dormant ended later than both real endings, independent of clock precision (§FS-rhei-retry.2).
    let active = record(&active_path);
    let mut dormant = record(&dormant_path);
    let latest_ended = [&active, &dormant]
        .into_iter()
        .map(|record| {
            time::OffsetDateTime::parse(
                record["ended"].as_str().unwrap(),
                &time::format_description::well_known::Rfc3339,
            )
            .unwrap()
        })
        .max()
        .unwrap();
    let newer = latest_ended + time::Duration::seconds(1);
    dormant["ended"] = super::provider_limit_support::utc_at(newer.unix_timestamp() as u64).into();
    fs::write(&dormant_path, serde_json::to_vec_pretty(&dormant).unwrap()).unwrap();
    let dormant_before = fs::read(&dormant_path).unwrap();
    let dormant = record(&dormant_path);
    let active = record(&active_path);
    assert_eq!(dormant["moves"], active["moves"]);
    assert_eq!(active["moves"], 0);
    assert_eq!(active["attempt"], 1);
    assert_eq!(active["charged"], 1);
    assert_eq!(dormant["attempt"], 2);
    assert_eq!(dormant["charged"], 2);
    assert_eq!(dormant["grants"].as_array().unwrap().len(), 1);
    // Prove active-runtime precedence against a strictly later dormant ended (§FS-rhei-retry.2).
    assert!(dormant["ended"].as_str().unwrap() > active["ended"].as_str().unwrap());
    let plan_path = member.join("index.rhei.md");
    let task_path = member.join("tasks/01-work.md");
    let before_plan = fs::read(&plan_path).unwrap();
    let before_task = fs::read(&task_path).unwrap();
    let granted = fixture.retry();
    assert_success(&granted);
    let output = text(&granted);
    assert!(output.contains("granted 1 more attempts"), "{output}");
    assert!(output.contains("(2 in total)"), "{output}");
    // The active record supplies the promised next attempt (§FS-rhei-retry.3).
    assert!(output.contains("attempt 2"), "{output}");
    assert!(!output.contains("attempt 3"), "{output}");
    let selected = record(&active_path);
    for field in ["moves", "attempt", "charged"] {
        assert_eq!(selected[field], active[field]);
    }
    assert_eq!(selected["grants"].as_array().unwrap().len(), 1);
    assert_eq!(selected["grants"][0]["attempts"], 1);
    assert_eq!(fs::read(&dormant_path).unwrap(), dormant_before);
    assert_eq!(fs::read(&plan_path).unwrap(), before_plan);
    assert_eq!(fs::read(&task_path).unwrap(), before_task);
    let granted_bytes = fs::read(&active_path).unwrap();
    let refusal = fixture.retry();
    assert!(!refusal.status.success());
    assert!(text(&refusal).contains("1 attempts left"), "{}", text(&refusal));
    assert_eq!(fs::read(&active_path).unwrap(), granted_bytes);
    assert_eq!(fs::read(&dormant_path).unwrap(), dormant_before);
    assert_eq!(fs::read(&plan_path).unwrap(), before_plan);
    assert_eq!(fs::read(&task_path).unwrap(), before_task);
    write_fixture_file(&member, "recover", "yes");
    let resumed = fixture.run();
    assert_success(&resumed);
    assert!(!text(&resumed).contains("1 attempts spent"), "{}", text(&resumed));
    let consumed = record(&active_path);
    assert_eq!(consumed["attempt"], 2);
    assert_eq!(consumed["charged"], 2);
    assert_eq!(consumed["moves"], selected["moves"]);
    assert_eq!(consumed["grants"], selected["grants"]);
    assert_eq!(fs::read(&dormant_path).unwrap(), dormant_before);
    assert_eq!(fs::read_to_string(member.join("attempts.log")).unwrap(), "1:1\n1:1\n1:2\n1:2\n");
    assert!(fixture
        .project
        .join("runtime/logs/task-follow.1-work-mock-openai-model-a-attempt2.log")
        .is_file());
    assert!(fs::read_to_string(&task_path).unwrap().contains("**State:** done"));
}

#[test]
fn retry_grants_project_record_over_newer_readmitted_member_record() {
    assert_recovery(false);
}

#[test]
fn retry_grants_project_record_consumed_by_parallel_run() {
    assert_recovery(true);
}
