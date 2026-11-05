//! Reading the `rhei reset` a message offers and running it as printed, and the
//! projects whose spawn is refused for a log no record accounts for.
//!
//! The command is read out of what rhei printed and handed to the platform's own
//! shell from a directory that is not the run's. Nothing here compares it to a
//! string: "runs as printed" is the property, and a string match would pin only
//! the spelling.

// §FS-rhei-reset.5.1 §FS-rhei-agents.8.1

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use super::entry_log_support::settings;
use super::*;

/// Every `rhei reset` the text offers, as printed: whatever a backticked span
/// holds, or, outside any backticks, the rest of the line. A reader copies both.
pub(super) fn offered_resets(text: &str) -> Vec<String> {
    let mut offers = Vec::new();
    for line in text.lines() {
        for (index, span) in line.split('`').enumerate() {
            let backticked = index % 2 == 1;
            if backticked {
                let words: Vec<&str> = span.split_whitespace().collect();
                if words.first() == Some(&"rhei") && words.contains(&"reset") {
                    offers.push(span.to_string());
                }
            } else if let Some(at) = span.find("rhei reset") {
                offers.push(span[at..].trim().to_string());
            }
        }
    }
    offers
}

/// Every `rhei transition` the text offers in backticks, as printed.
pub(super) fn offered_transitions(text: &str) -> Vec<String> {
    let spans = text.lines().flat_map(|line| line.split('`').skip(1).step_by(2));
    spans.filter(|span| span.starts_with("rhei transition ")).map(str::to_string).collect()
}

/// The word after `flag` in an offered command, as printed.
pub(super) fn offered_flag<'a>(offer: &'a str, flag: &str) -> Option<&'a str> {
    let mut words = offer.split_whitespace();
    words.find(|word| *word == flag)?;
    words.next()
}

/// The state an offered `rhei transition` goes to, as printed.
pub(super) fn offered_to(offer: &str) -> Option<&str> {
    offered_flag(offer, "--to")
}

/// The way back a finished ticket's refusal offers ahead of the reset, returned as
/// printed. §FS-rhei-reset.5.1
#[track_caller]
pub(super) fn assert_offers_forced_reopen(said: &str, home: &Path) -> String {
    let offer = assert_offers_forced_line(said, home);
    let line = said
        .lines()
        .find(|line| line.contains(offer.as_str()))
        .expect("the offer was read from one line");
    let reset_at = line.find("`rhei reset ").expect("the reset is offered on the same line");
    assert!(
        line.find(offer.as_str()).is_some_and(|at| at < reset_at),
        "the one-ticket way back comes before the reset:\n{line}"
    );
    offer
}

/// The forced transition a finished ticket's refusal offers, returned as printed: it
/// runs as printed and stops at the forced path's own terminal check, which comes
/// after every stable preflight, so the shell, the arguments and the hop were all
/// accepted, on every platform and with no pseudo-terminal. The line says who it is
/// for, and that the printed reason is theirs to replace.
/// §FS-rhei-reset.5.1 §FS-rhei-transition-cmd.6
#[track_caller]
pub(super) fn assert_offers_forced_line(said: &str, home: &Path) -> String {
    let offers = offered_transitions(said);
    let [offer] = offers.as_slice() else {
        panic!("one `rhei transition` must be offered, found {}, in:\n{said}", offers.len());
    };

    let literal = run_offer(offer, "", home);
    let refusal = format!("{}{}", literal.stdout, literal.stderr);
    assert!(
        !literal.status.success()
            && refusal
                .contains("--force requires an interactive terminal and fresh typed confirmation"),
        "as printed, with no terminal, it stops at the forced path's own confirmation.\n{offer} \
         exited {:?}:\n{refusal}",
        literal.status.code()
    );

    let line = said
        .lines()
        .find(|line| line.contains(offer.as_str()))
        .expect("the offer was read from one line");
    assert!(line.contains("An operator at a terminal"), "it says who may run it:\n{line}");
    assert!(line.contains("an agent may not run"), "it says an agent may not:\n{line}");
    assert!(
        line.contains(&format!("`{offer}`, giving their own reason")),
        "it says the printed reason is the operator's to replace:\n{line}"
    );
    offer.clone()
}

/// The `rhei complete` a finished ticket's refusal answers, with its help's commands
/// held whole on the line they were printed on, in the bytes as written: `CliRun`
/// undoes soft wraps, so only the raw reading can see this. `commands` is how many it
/// offers, each wider than the eighty columns captured stderr renders at, whatever
/// the temporary directory is, so without the exception no runner could keep them
/// whole. §FS-rhei-reset.5.1 §FS-rhei-errors.2
#[track_caller]
pub(super) fn assert_finished_tickets_commands_stay_whole(world: &RemedyWorld, commands: usize) {
    let refused =
        world.rhei_output("complete", &["--task", "1", "--result", "again", "--no-callbacks"]);

    let rendered = raw_stderr(&refused);
    let unwrapped = stderr(&refused);
    assert!(
        unwrapped.contains("already in terminal state"),
        "fixture: this is the refusal under test:\n{unwrapped}"
    );
    let offered = [offered_resets(&unwrapped), offered_transitions(&unwrapped)].concat();
    assert_eq!(offered.len(), commands, "the commands offered in:\n{unwrapped}");
    for command in offered {
        assert!(command.len() > 80, "fixture: {command} must not fit the render");
        assert!(
            rendered.lines().any(|line| line.contains(&format!("`{command}`"))),
            "`{command}` must arrive on one line, and was rendered:\n{rendered}"
        );
    }
}

/// Run `line` and what follows it through the platform's shell, from a directory
/// of its own, with `rhei` on the PATH and nothing inherited from a surrounding
/// run. A reader's shell is what the command was printed for, so its quoting is
/// under test too.
pub(super) fn run_offer(line: &str, suffix: &str, home: &Path) -> CliRun {
    let elsewhere = unique_temp_dir("reset-remedy-elsewhere");
    let mut command = rhei_core::platform::system_shell_command(&format!("{line} {suffix}"));
    let binary = rhei_binary();
    let directory = binary.parent().expect("the binary sits in a directory").to_path_buf();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let paths = std::iter::once(directory).chain(std::env::split_paths(&path));
    command
        .current_dir(&*elsewhere)
        .stdin(Stdio::null())
        .env("PATH", std::env::join_paths(paths).expect("a joinable PATH"))
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join("state"))
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE");
    scrub_inherited_rhei_env(&mut command);
    CliRun::from(&command.output().expect("the offered command should start"))
}

fn scrub_inherited_rhei_env(command: &mut Command) {
    for (name, _) in std::env::vars_os() {
        let name_text = name.to_string_lossy();
        if name_text.starts_with("RHEI_") || name_text.starts_with("GIT_") {
            command.env_remove(&name);
        }
    }
}

/// The whole of §FS-rhei-reset.5.1 for one surface: every reset `said` offers
/// runs as printed from elsewhere and stops at reset's own confirmation, its
/// preview stays inside `rhei` and names every ticket in `moved`, and the line
/// offering it is in backticks and says how far it reaches.
#[track_caller]
pub(super) fn assert_offers_runnable_reset(said: &str, home: &Path, rhei: &str, moved: &[&str]) {
    let offers = offered_resets(said);
    assert!(!offers.is_empty(), "no `rhei reset` is offered in:\n{said}");
    for offer in &offers {
        let preview = run_offer(offer, "--dry-run", home);
        assert!(
            preview.status.success(),
            "the offered command must run as printed.\n{offer} --dry-run, from another \
             directory, exited {:?}:\n{}{}\noffered in:\n{said}",
            preview.status.code(),
            preview.stdout,
            preview.stderr
        );
        assert_preview_stays_in(&preview.stdout, offer, rhei, moved);

        let literal = run_offer(offer, "", home);
        let refusal = format!("{}{}", literal.stdout, literal.stderr);
        assert!(
            !literal.status.success() && refusal.contains("stdin is not a terminal"),
            "as printed, with no terminal, it stops at reset's own confirmation \
             (§FS-rhei-reset.1.2), so it carries no --yes.\n{offer} exited {:?}:\n{refusal}",
            literal.status.code()
        );

        let line = said
            .lines()
            .find(|line| line.contains(offer.as_str()))
            .expect("the offer was read from one line");
        assert!(line.contains(&format!("`{offer}`")), "the command is in backticks:\n{line}");
        assert!(
            line.contains("every ticket in that rhei"),
            "the line says how far the command reaches:\n{line}"
        );
    }
}

fn assert_preview_stays_in(preview: &str, offer: &str, rhei: &str, moved: &[&str]) {
    let narrowed = format!("narrowed to 1 rhei: {rhei}");
    for line in preview.lines().filter(|line| line.starts_with("Scope:")) {
        assert!(line.ends_with(&narrowed), "{offer} is not scoped to {rhei}:\n{preview}");
    }
    let own = format!("Task {rhei}.");
    for line in preview.lines().map(str::trim).filter(|line| line.starts_with("Task ")) {
        assert!(line.starts_with(&own), "{offer} reaches beyond {rhei}:\n{preview}");
    }
    assert!(
        !preview.contains("no longer declares"),
        "{offer} resolves another machine than the run's, so it cannot move a ticket \
         back:\n{preview}"
    );
    for ticket in moved {
        assert!(
            preview.contains(&format!("Task {ticket}: ")),
            "{offer} would move {ticket} back, and its preview must say so:\n{preview}"
        );
    }
}

/// `open` is the authored state and a gate, so a ticket that left it shows up
/// in a reset's move list. `measure` is a program and `work` an agent, because
/// their logs land under different roots (§FS-rhei-reset.2.1).
pub(super) fn remedy_machine(measure: &Path) -> String {
    format!(
        r#"name: reset-remedy
version: 1
states:
  open:
    initial: true
    gating: true
    description: Not started
  measure:
    description: A program's state
    program:
      command: {command}
    program_timeout: {FIXTURE_MARGIN}
  work:
    description: An agent's state
    agent: mock
    agent_timeout: {FIXTURE_MARGIN}
  waiting:
    gating: true
    description: Waits here with its work done
  cancelled:
    final: true
    description: Dropped
transitions:
  - {{ from: open, to: measure, description: Start the program }}
  - {{ from: open, to: work, description: Start the agent }}
  - {{ from: measure, to: waiting, exit_code: 0, description: Measured }}
  - {{ from: work, to: waiting, description: Worked }}
  - {{ from: "*", to: cancelled, description: Dropped }}
"#,
        command = fixture_command(measure)
    )
}

/// One body for the program and the agent: both leave a result for a reset to lose.
const WORKER: &str = r#"sys.stdout.write('WORKED-{}\n'.format(env('RHEI_TASK_ID')))
result('worked\n')
"#;

pub(super) const TWO_TICKETS: &str = r#"# Rhei: Reset remedy

## Tasks

### Task 1: The ticket the refusal is about
**State:** open

### Task 2: A ticket the refusal is not about
**State:** open
"#;

/// A tree holding a message that offers reset, and the invocation that printed
/// it: what the command is pointed at, from where, and under which machine.
pub(super) struct RemedyWorld {
    /// Keeps the tree alive for as long as the test holds the world.
    pub(super) dir: TestDir,
    pub(super) home: PathBuf,
    /// The directory holding the machine, the settings and the worker's script.
    pub(super) root: PathBuf,
    pub(super) cwd: PathBuf,
    pub(super) target: PathBuf,
    /// `--state-machine` as the run spells it, when it names one.
    pub(super) machine: Option<PathBuf>,
    /// `--parallel`'s spawn sites are their own code.
    pub(super) parallel: bool,
}

impl RemedyWorld {
    /// `root` under a directory of its own, with the run started on it from
    /// inside it. A test swaps what differs with struct update syntax.
    pub(super) fn at(name: &str, relative_root: &str) -> Self {
        let dir = unique_temp_dir(name);
        let root = dir.join(relative_root);
        fs::create_dir_all(&root).expect("create the root");
        let home = dir.join(".home");
        RemedyWorld {
            cwd: root.clone(),
            target: root.clone(),
            root,
            home,
            dir,
            machine: None,
            parallel: false,
        }
    }

    /// The machine beside the rhei or project, which is where a run finds it
    /// without being told.
    pub(super) fn with_found_machine(self) -> Self {
        self.write_machine_at(&self.root.join("states.yaml"));
        self
    }

    /// The machine at `path`, and what its states run: a worker beside the tree
    /// and the agent settings at `root`.
    pub(super) fn write_machine_at(&self, path: &Path) {
        let worker = write_python_agent(&self.dir, "worker.py", WORKER);
        fs::write(path, remedy_machine(&worker)).expect("write the machine");
        settings(&self.root, &worker);
    }

    pub(super) fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the parent");
        fs::write(path, contents).expect("write the fixture file");
    }

    /// `rhei <subcommand> <target> <args>`, as this world's run spells it.
    pub(super) fn rhei(&self, subcommand: &str, args: &[&str]) -> CliRun {
        CliRun::from(&self.rhei_output(subcommand, args))
    }

    /// The same run with its bytes as written, for a test about where lines fall.
    pub(super) fn rhei_output(&self, subcommand: &str, args: &[&str]) -> Output {
        let mut command = rhei_command(&self.home);
        command.current_dir(&self.cwd);
        scrub_inherited_rhei_env(&mut command);
        if let Some(machine) = &self.machine {
            command.arg("--state-machine").arg(machine);
        }
        command.arg(subcommand).arg(&self.target).args(args);
        command.output().expect("rhei should start")
    }

    pub(super) fn step(&self, subcommand: &str, args: &[&str]) {
        let run = self.rhei(subcommand, args);
        assert!(
            run.status.success(),
            "fixture step `rhei {subcommand} {args:?}` failed:\n{}{}",
            run.stdout,
            run.stderr
        );
    }

    pub(super) fn run(&self) -> CliRun {
        let mut args = vec!["--no-tui", "--no-callbacks"];
        if self.parallel {
            args.extend(["--parallel", "2"]);
        }
        self.rhei("run", &args)
    }

    /// Send `ticket` into `state` and let its worker finish there.
    pub(super) fn work(&self, ticket: &str, state: &str) {
        self.enter(ticket, state);
        self.run();
        assert_eq!(self.state_of(ticket), "waiting", "fixture: {ticket} did its work");
    }

    pub(super) fn enter(&self, ticket: &str, state: &str) {
        let args = ["--task", ticket, "--from", "open", "--to", state, "--no-callbacks"];
        self.step("transition", &args);
    }

    pub(super) fn state_of(&self, ticket: &str) -> String {
        let listing = self.rhei("list", &[]);
        let prefix = format!("Task {ticket}: ");
        let line = listing
            .stdout
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("{ticket} is not listed:\n{}", listing.stdout));
        let (_, state) = line.rsplit_once('[').expect("a state in brackets");
        state.trim_end_matches(']').to_string()
    }

    /// Put a file at `ticket`'s log name for `state`, under `log_root`, and run:
    /// what the run said, once the fixture has shown it is the refusal under test.
    #[track_caller]
    pub(super) fn refuse(&self, ticket: &str, state: &str, log_root: &Path) -> String {
        self.enter(ticket, state);
        let logs = log_root.join("runtime/logs");
        fs::create_dir_all(&logs).expect("create the log directory");
        let planted = logs.join(format!("task-{ticket}-{state}.log"));
        fs::write(&planted, "A log no spawn record accounts for.\n").expect("plant the log");

        let run = self.run();
        let said = format!("{}{}", run.stdout, run.stderr);
        assert!(
            said.contains("refusing to spawn: ")
                && said.contains("exists and no spawn record accounts for it"),
            "fixture: the run must refuse over {}; it said:\n{said}",
            planted.display()
        );
        assert!(said.contains("move that file away"), "the cheap way out comes first:\n{said}");
        assert_eq!(
            fs::read_to_string(&planted).expect("the planted log"),
            "A log no spawn record accounts for.\n",
            "the refusal keeps the file"
        );
        assert_eq!(self.state_of(ticket), state, "the refusal leaves the ticket where it is");
        said
    }

    /// The durable run report, which is read long after the run and from
    /// anywhere; empty if the run wrote none.
    pub(super) fn report(&self) -> String {
        fs::read_to_string(self.root.join("runtime/run-report.md")).unwrap_or_default()
    }
}
