//! Shared setup and acceptance for the portable and controlled publication
//! scenarios. §FS-rhei-run.3.7.3 §FS-rhei-run.3.7.4

use std::fs;
use std::path::PathBuf;

use super::python_fixture::fixture_command_with_args;
use super::worker_edit_revert_prompt_tests::{
    block, one_file_machine, LATER_FILE, ONE_FILE_COVER, ONE_FILE_OTHER, SHARED_FILE,
};
use super::worker_edit_revert_support::{end_records, meta_value, state_in, Mode};
use super::*;

pub(super) struct OneFileScenario {
    pub dir: TestDir,
    pub ws: PathBuf,
    pub machine: PathBuf,
    pub mode: Mode,
}

impl OneFileScenario {
    /// Instrumentation is prepended to the same fixture body, never an
    /// alternative publisher. Implement repairs ONE_FILE_COVER itself.
    pub fn new(prefix: &str, mode: Mode, instrumentation: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let ws = dir.join("ws");
        fs::create_dir_all(ws.join("tasks")).expect("create workspace");
        fs::write(ws.join("index.rhei.md"), "# Rhei: One file\n").expect("write index");
        write_fixture_file(&ws.join("tasks"), "01-shared.md", SHARED_FILE);
        write_fixture_file(&ws.join("tasks"), "02-later.md", LATER_FILE);
        let cover =
            write_python_agent(&dir, "cover.py", &format!("{instrumentation}\n{ONE_FILE_COVER}"));
        let other = write_python_agent(&dir, "other.py", ONE_FILE_OTHER);
        let concurrent = if mode.concurrent() { "concurrent" } else { "alone" };
        let machine = write_fixture_file(
            &ws,
            "states.yaml",
            &one_file_machine(
                &fixture_command_with_args(&cover, &[concurrent]),
                &fixture_command(&other),
            ),
        );
        Self { dir, ws, machine, mode }
    }

    pub fn task_file(&self, name: &str) -> String {
        fs::read_to_string(self.ws.join("tasks").join(name)).unwrap_or_default()
    }

    pub fn journal(&self) -> String {
        fs::read_to_string(self.ws.join("runtime/transitions.log")).unwrap_or_default()
    }

    /// Both authored files and the accounting evidence accompany any failure.
    pub fn diagnostics(&self, ran: &CliRun) -> String {
        let mut output = format!(
            "mode {:?}; fixture {}\nstdout:\n{}\nstderr:\n{}",
            self.mode,
            self.dir.display(),
            ran.stdout,
            ran.stderr
        );
        for file in [
            "tasks/01-shared.md",
            "tasks/02-later.md",
            "runtime/transitions.log",
            "runtime/state-transitions.log",
            "runtime/spawns/task-ws.1-cover.json",
            "runtime/spawns/task-ws.2-other.json",
            "runtime/logs/task-ws.1-cover.reverted.md",
        ] {
            let text = fs::read_to_string(self.ws.join(file)).unwrap_or_default();
            output.push_str(&format!("\n{file}:\n{text}"));
        }
        output
    }

    pub fn assert_sibling_preserved(&self, output: &str) {
        let text = self.task_file("01-shared.md");
        let sibling = block(&text, "### Task 2:");
        assert_eq!(state_in(sibling), "completed", "{output}");
        assert!(
            sibling.contains("> **Result:** [ws.2](runtime/results/ws.2.md)"),
            "Task 2 keeps its finalized result link\n{output}"
        );
        let result =
            fs::read_to_string(self.ws.join("runtime/results/ws.2.md")).unwrap_or_default();
        assert!(result.contains("Task 2 did its unrelated work."), "{output}");
        let runs = fs::read_to_string(self.dir.join("other-runs.txt")).unwrap_or_default();
        assert_eq!(runs.lines().count(), 1, "Task 2 ran once\n{output}");
        let later = self.task_file("02-later.md");
        assert!(later.contains("### Task 3:"), "{output}");
        assert!(later.contains("**Prior:** Task 2"), "{output}");
    }

    pub fn assert_completed(&self, ran: &CliRun) {
        let output = self.diagnostics(ran);
        assert!(!ran.stderr.contains("Falling back to sequential"), "{output}");
        assert!(ran.status.success(), "the run survives the edit\n{output}");
        self.assert_sibling_preserved(&output);
        let text = self.task_file("01-shared.md");
        assert_eq!(state_in(block(&text, "### Task 1:")), "completed", "{output}");
        assert_eq!(state_in(&self.task_file("02-later.md")), "completed", "{output}");
        assert!(!text.contains("#### Visit"), "{output}");
        let journal = self.journal();
        let ends = end_records(&journal, "ws.1", "cover");
        assert_eq!(ends.len(), 2, "{output}");
        assert_eq!(meta_value(&ends[0], "outcome").as_deref(), Some("failed"), "{output}");
        assert!(meta_value(&ends[0], "reverted").is_some(), "{output}");
        assert_eq!(meta_value(&ends[1], "outcome").as_deref(), Some("completed"), "{output}");
        let spawn: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(self.ws.join("runtime/spawns/task-ws.1-cover.json"))
                .expect("Task 1 spawn"),
        )
        .expect("spawn JSON");
        assert_eq!(spawn["attempt"], 2, "{output}");
        assert_eq!(spawn["charged"], 2, "the bad attempt consumed one charge\n{output}");
        assert_eq!(spawn["attempt_charged"], true, "{output}");
        let reverted = fs::read_to_string(self.ws.join("runtime/logs/task-ws.1-cover.reverted.md"))
            .unwrap_or_default();
        assert!(reverted.contains("#### Visit 1 (cover)"), "{output}");
        assert!(reverted.contains("Latest measurement was report-1.json."), "{output}");
        let transitions =
            fs::read_to_string(self.ws.join("runtime/state-transitions.log")).unwrap_or_default();
        let own: Vec<_> = transitions.lines().filter(|line| line.contains("ws.1")).collect();
        assert_eq!(own.len(), 1, "only the successful retry transitions\n{output}");
    }
}
