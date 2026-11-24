//! Which tickets share a non-concurrent state's one slot in a project run
//! (#759): those of the same state of the same machine source, and no others.
//!
//! Two rheis that each carry their own `states.yaml` with a state of the same
//! name hold two states, so a ticket in one never waits for the other. Two
//! tickets of one machine, or of two rheis governed by the project root's one
//! file, still wait for each other.
//!
//! The stand-in proves overlap without sleeping on a guess: a spawn that has a
//! peer waits (bounded) for the peer to arrive and records whether it came, so
//! a deferral shows as `alone` rather than as a timing. A spawn with no peer
//! holds its slot a moment and records how many of its group were alive.

// §FS-rhei-run.5.2.1 §FS-rhei-run.5.2

use std::fs;
use std::path::Path;

use super::*;

/// One machine, written once per rhei that carries its own file and once at
/// the project root for the shared case. `polish` and `collect` leave
/// `concurrent` unset, so each is non-concurrent; `steer` leads into `polish`.
fn machine(name: &str, program: &str) -> String {
    format!(
        r#"name: {name}
version: 1
states:
  steer: {{ description: Steer it, target: "mock:mock:stub" }}
  polish: {{ description: Polish it, target: "mock:mock:stub" }}
  collect: {{ description: Collect it, program: {{ command: {program} }} }}
  done: {{ final: true, description: Done }}
  cancelled: {{ final: true, description: Dropped }}
transitions:
  - {{ from: steer, to: polish }}
  - {{ from: polish, to: done }}
  - {{ from: collect, to: done }}
  - {{ from: "*", to: cancelled }}
"#
    )
}

/// Every spawn, agent or program. `PEERS` maps `<task> <state>` to the spawn it
/// waits (at most `WAIT` s) to see arrive; any other spawn holds for `HOLD` s.
/// Each records `<task> <state> <together|alone|held> <rhei-alive> <state-alive>`,
/// the most of its rhei's and of all rheis' spawns in that state alive at once.
const AGENT: &str = r#"shared = pathlib.Path(SHARED)
task, state = env('RHEI_TASK_ID'), env('RHEI_STATE')
slug = lambda text: re.sub(r'[^A-Za-z0-9]+', '-', text)
key = task + ' ' + state
arrived = shared / 'arrived'
arrived.mkdir(parents=True, exist_ok=True)
by_rhei = shared / 'live' / slug(task.split('.', 1)[0] + '-' + state)
by_state = shared / 'live' / ('all-' + state)
for live in (by_rhei, by_state):
    live.mkdir(parents=True, exist_ok=True)
    (live / slug(task)).touch()
(arrived / slug(key)).touch()
count = lambda: (len(list(by_rhei.iterdir())), len(list(by_state.iterdir())))
seen = count()
peer = PEERS.get(key)
if peer:
    until = time.monotonic() + WAIT
    while not (arrived / slug(peer)).exists() and time.monotonic() < until:
        time.sleep(.025)
    outcome = 'together' if (arrived / slug(peer)).exists() else 'alone'
else:
    time.sleep(HOLD)
    outcome = 'held'
seen = tuple(max(a, b) for a, b in zip(seen, count()))
for live in (by_rhei, by_state):
    (live / slug(task)).unlink()
append(shared / 'spawns.log', '%s %s %d %d\n' % (key, outcome, seen[0], seen[1]))
result('done\n')
"#;

/// A rhei: its name, whether it carries its own `states.yaml`, and its tickets
/// as `(id, state)`.
type Rhei<'a> = (&'a str, bool, &'a [(&'a str, &'a str)]);

/// A project under `dir`: `rheis` names each rhei with its tickets
/// (`(id, state)`), and whether it carries its own `states.yaml`; a rhei that
/// does not runs under the project root's file.
struct Project<'a> {
    rheis: &'a [Rhei<'a>],
    peers: &'a [(&'a str, &'a str)],
    wait: u32,
}

/// What a project run left behind: its output and one record per spawn.
struct Ran {
    output: String,
    spawns: Vec<Vec<String>>,
}

impl Ran {
    fn spawn(&self, task: &str, state: &str) -> &Vec<String> {
        self.spawns
            .iter()
            .find(|fields| fields[0] == task && fields[1] == state)
            .unwrap_or_else(|| panic!("{task} never ran in {state}; output:\n{}", self.output))
    }

    fn assert_together(&self, task: &str, state: &str) {
        let outcome = &self.spawn(task, state)[2];
        assert_eq!(
            outcome, "together",
            "{task} in {state} waited alone for its peer in another rhei's machine, so the \
             two never overlapped; spawns: {:?}\noutput:\n{}",
            self.spawns, self.output
        );
    }

    fn assert_not_deferred(&self, task: &str) {
        let deferred = self.output.lines().find(|l| l.starts_with("Deferred") && l.contains(task));
        assert_eq!(deferred, None, "{task} was deferred; output:\n{}", self.output);
    }

    /// The most spawns alive at once in `state`: column 3 counts one rhei's, 4 every rhei's.
    fn most_alive(&self, state: &str, column: usize) -> usize {
        self.spawns
            .iter()
            .filter(|fields| fields[1] == state)
            .map(|fields| fields[column].parse::<usize>().expect("a count"))
            .max()
            .unwrap_or(0)
    }
}

fn run_project(prefix: &str, project: &Project) -> Ran {
    let dir = unique_temp_dir(prefix);
    let root = dir.join("project");
    let shared = dir.join("shared");
    fs::create_dir_all(&root).expect("create project");
    fs::write(root.join("index.panta.md"), "# Panta: Side by side\n").expect("write index");
    let peers: Vec<String> =
        project.peers.iter().map(|(from, to)| format!("{from:?}: {to:?}")).collect();
    let preamble = format!(
        "SHARED = {shared}\nPEERS = {{{peers}}}\nWAIT = {wait}\nHOLD = 1.5\n",
        shared = serde_json::to_string(&shared.display().to_string()).expect("quote"),
        peers = peers.join(", "),
        wait = project.wait,
    );
    let agent = write_python_agent(&dir, "stand_in.py", &format!("{preamble}{AGENT}"));
    let command = fixture_command(&agent);
    fs::write(root.join("states.yaml"), machine("project-machine", &command)).expect("machine");
    for (rhei, own_machine, tickets) in project.rheis {
        let tasks = root.join(rhei).join("tasks");
        fs::create_dir_all(&tasks).expect("create rhei");
        fs::write(root.join(rhei).join("index.rhei.md"), format!("# Rhei: {rhei}\n"))
            .expect("write rhei index");
        if *own_machine {
            let name = format!("{rhei}-machine");
            fs::write(root.join(rhei).join("states.yaml"), machine(&name, &command))
                .expect("write rhei machine");
        }
        // One file per ticket, so no shared task file holds the run to one slot (§FS-rhei-run.2.5).
        for (id, state) in *tickets {
            let body =
                format!("### Task {id}: Deliver {rhei} {id}\n**State:** {state}\n\nDo it.\n");
            fs::write(tasks.join(format!("{id}.md")), body).expect("write ticket");
        }
    }
    write_settings(&root, &command);

    let run =
        run_cli_without_machine("run", &root, &["--no-tui", "--no-dashboard", "--parallel", "4"]);
    let output = format!("{}\n{}", run.stdout, run.stderr);
    assert!(run.status.success(), "the run failed; output:\n{output}");
    let tickets: usize = project.rheis.iter().map(|(_, _, t)| t.len()).sum();
    let all_terminal = format!("{tickets}/{tickets} tasks in terminal state");
    assert!(output.contains(&all_terminal), "every ticket finishes; output:\n{output}");
    let spawns = fs::read_to_string(shared.join("spawns.log"))
        .unwrap_or_default()
        .lines()
        .map(|line| line.split(' ').map(str::to_string).collect())
        .collect();
    Ran { output, spawns }
}

fn write_settings(root: &Path, command: &str) {
    let settings = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("create settings dir");
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "60s" }},
  "agents": {{ "mock": {{ "command": {command}, "stdin_prompt": true, "timeout": "60s" }} }},
  "models": {{ "stub": {{ "provider": "mock", "model": "stub", "default_agent": "mock" }} }}
}}"#
        ),
    )
    .expect("write settings");
}

/// Two rheis, each with its own machine and one ticket ready in its own
/// non-concurrent agent state `polish`: both start in the first fill and run
/// side by side. §FS-rhei-run.5.2.1
#[test]
fn same_named_agent_states_of_two_machines_start_together() {
    let ran = run_project(
        "nonconcurrent-agents",
        &Project {
            rheis: &[
                ("alpha", true, &[("deliver", "polish")]),
                ("beta", true, &[("deliver", "polish")]),
            ],
            peers: &[
                ("alpha.deliver polish", "beta.deliver polish"),
                ("beta.deliver polish", "alpha.deliver polish"),
            ],
            wait: 10,
        },
    );
    ran.assert_together("alpha.deliver", "polish");
    ran.assert_together("beta.deliver", "polish");
    ran.assert_not_deferred("alpha.deliver");
    ran.assert_not_deferred("beta.deliver");
}

/// The same for a program state: `collect` in two machines is two states.
/// §FS-rhei-run.5.2.1
#[test]
fn same_named_program_states_of_two_machines_start_together() {
    let ran = run_project(
        "nonconcurrent-programs",
        &Project {
            rheis: &[
                ("alpha", true, &[("collect", "collect")]),
                ("beta", true, &[("collect", "collect")]),
            ],
            peers: &[
                ("alpha.collect collect", "beta.collect collect"),
                ("beta.collect collect", "alpha.collect collect"),
            ],
            wait: 10,
        },
    );
    ran.assert_together("alpha.collect", "collect");
    ran.assert_together("beta.collect", "collect");
    ran.assert_not_deferred("alpha.collect");
    ran.assert_not_deferred("beta.collect");
}

/// The refill: `beta.deliver` leaves `steer` for its own `polish` while
/// `alpha.deliver` is still in `alpha`'s `polish`, and starts at once rather
/// than after it. §FS-rhei-run.5.2.1
#[test]
fn a_ticket_entering_a_same_named_state_of_its_own_machine_starts_in_the_refill() {
    let ran = run_project(
        "nonconcurrent-refill",
        &Project {
            rheis: &[
                ("alpha", true, &[("deliver", "polish")]),
                ("beta", true, &[("deliver", "steer")]),
            ],
            peers: &[
                ("alpha.deliver polish", "beta.deliver polish"),
                ("beta.deliver polish", "alpha.deliver polish"),
            ],
            wait: 15,
        },
    );
    ran.spawn("beta.deliver", "steer");
    ran.assert_together("alpha.deliver", "polish");
    ran.assert_not_deferred("beta.deliver");
}

/// Two tickets of one rhei in its non-concurrent `polish` still take turns,
/// whatever another rhei's same-named state does. §FS-rhei-run.5.2
#[test]
fn two_tickets_of_one_machine_in_a_nonconcurrent_state_still_take_turns() {
    let ran = run_project(
        "nonconcurrent-one-machine",
        &Project {
            rheis: &[
                ("alpha", true, &[("deliver", "polish"), ("second", "polish")]),
                ("beta", true, &[("deliver", "polish")]),
            ],
            peers: &[],
            wait: 0,
        },
    );
    assert_eq!(ran.spawns.len(), 3, "every ticket ran once; output:\n{}", ran.output);
    assert_eq!(
        ran.most_alive("polish", 3),
        1,
        "alpha's two polish tickets overlapped:\n{}",
        ran.output
    );
}

/// Two rheis with no machine of their own both run under the project root's
/// one file, so their `polish` is one state and its tickets take turns.
/// §FS-rhei-run.5.2.1
#[test]
fn rheis_governed_by_one_machine_file_share_its_nonconcurrent_state() {
    let ran = run_project(
        "nonconcurrent-shared-machine",
        &Project {
            rheis: &[
                ("alpha", false, &[("deliver", "polish")]),
                ("beta", false, &[("deliver", "polish")]),
            ],
            peers: &[],
            wait: 0,
        },
    );
    assert_eq!(ran.spawns.len(), 2, "every ticket ran once; output:\n{}", ran.output);
    assert_eq!(
        ran.most_alive("polish", 4),
        1,
        "two rheis under the project's one machine ran its non-concurrent polish at once:\n{}",
        ran.output
    );
}
