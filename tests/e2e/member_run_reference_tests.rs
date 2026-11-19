//! Member paths control the recorded project run, including member-scoped runs.
//! §FS-rhei-run-headless.3

use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::headless_support::{wait_until, Workspace};
use super::{stderr, stdout};

struct ProjectRun {
    ws: Workspace,
    id: String,
}

impl ProjectRun {
    fn new() -> Self {
        let ws = Workspace::new("rhei-746-member-run", 120);
        fs::remove_file(ws.plan()).unwrap();
        fs::write(ws.root.join("index.panta.md"), "# Panta: Demo\n").unwrap();
        fs::create_dir_all(ws.root.join("streams/tasks")).unwrap();
        fs::write(ws.root.join("streams/index.rhei.md"), "# Rhei: Streams\n").unwrap();
        fs::write(
            ws.root.join("streams/tasks/1.md"),
            "### Task 1: Work\n**State:** pending\n\nWork.\n",
        )
        .unwrap();
        Self::launch(ws, "streams")
    }

    fn launch(ws: Workspace, input: &str) -> Self {
        let mut launch = ws.rhei_command(&["run", input, "--headless", "--json", "--no-dashboard"]);
        launch.env("XDG_CONFIG_HOME", ws.home.join(".config"));
        let out = launch.output().unwrap();
        assert!(out.status.success(), "launch: {}", stderr(&out));
        let record: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
        let id = record["id"].as_str().expect("launcher run id").to_string();
        let run = Self { ws, id };
        wait_until("member-scoped work to start", Duration::from_secs(15), || {
            fs::read_to_string(run.ws.root.join("runtime/events.jsonl"))
                .is_ok_and(|log| log.contains("slot_assigned"))
        });
        let descriptor = run.ws.descriptor().expect("project descriptor");
        assert_eq!(descriptor["id"], run.id);
        assert_eq!(descriptor["status"], "running");
        assert_eq!(
            std::path::Path::new(descriptor["workspace"].as_str().unwrap()),
            fs::canonicalize(&run.ws.root).unwrap()
        );
        assert!(!run.ws.root.join("streams/runtime/run.json").exists());
        let listed = run.command("runs", Some("--json")).output().unwrap();
        assert!(listed.status.success(), "runs: {}", stderr(&listed));
        assert!(stdout(&listed).contains(&run.id), "the recorded run must be listed live");
        run
    }

    fn command(&self, verb: &str, reference: Option<&str>) -> Command {
        let mut cmd = self.ws.rhei_command(&[verb]);
        cmd.env("XDG_CONFIG_HOME", self.ws.home.join(".config"));
        if let Some(reference) = reference {
            cmd.arg(reference);
        } else {
            cmd.current_dir(self.ws.root.join("streams"));
        }
        cmd
    }

    fn stop(&self, reference: Option<&str>) {
        let out = self.command("stop", reference).arg("--wait").output().unwrap();
        assert!(
            out.status.success(),
            "stop {reference:?} must reach project run {} (exit {}):\n{}{}",
            self.id,
            out.status,
            stdout(&out),
            stderr(&out)
        );
        let text = format!("{}{}", stdout(&out), stderr(&out));
        assert!(text.contains(&format!("Asked run {}", self.id)), "{text}");
        let descriptor = self.ws.descriptor().unwrap();
        assert_eq!(descriptor["id"], self.id);
        assert_eq!(descriptor["status"], "failed");
        assert_eq!(descriptor["exit_code"], 130);
        assert!(!self.ws.root.join("streams/runtime/run.json").exists());
    }

    // A live JSON replay proves attachment without opening a terminal. Bound
    // the follower and reap it even if lookup fails or no records ever arrive.
    fn attach(&self, reference: Option<&str>) {
        let output_path = self.ws.root.join("attach.jsonl");
        let error_path = self.ws.root.join("attach.stderr");
        let mut child = self
            .command("attach", reference)
            .arg("--json")
            .stdout(Stdio::from(fs::File::create(&output_path).unwrap()))
            .stderr(Stdio::from(fs::File::create(&error_path).unwrap()))
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            let text = fs::read_to_string(&output_path).unwrap();
            let first = text
                .lines()
                .next()
                .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok());
            if first.is_some_and(|record| record["event"] == "run_started")
                || Instant::now() >= deadline
            {
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let _ = child.kill();
        child.wait().unwrap();
        let text = fs::read_to_string(output_path).unwrap();
        let error = fs::read_to_string(error_path).unwrap();
        assert!(
            status.is_none() && text.contains("run_started"),
            "attach {reference:?} must follow project run {} (exit {status:?}):\n{text}{error}",
            self.id
        );
        let first: serde_json::Value =
            serde_json::from_str(text.lines().next().expect("first record")).unwrap();
        assert_eq!(first["event"], "run_started");
        assert_eq!(first["run_id"], self.id);
        assert_eq!(self.ws.descriptor().unwrap()["status"], "running");
        assert!(!self.ws.root.join("streams/runtime/run.json").exists());
    }
}

#[test]
fn member_run_746_standalone_controls() {
    for directory in [false, true] {
        let ws = Workspace::new("rhei-746-standalone", 120);
        let input = if directory {
            fs::remove_file(ws.plan()).unwrap();
            fs::create_dir_all(ws.root.join("tasks")).unwrap();
            fs::write(ws.root.join("index.rhei.md"), "# Rhei: Standalone\n").unwrap();
            fs::write(ws.root.join("tasks/1.md"), "### Task 1: Work\n**State:** pending\n")
                .unwrap();
            "."
        } else {
            "plan.rhei.md"
        };
        let run = ProjectRun::launch(ws, input);
        run.attach(Some(input));
        run.stop(Some(input));
    }
}

macro_rules! member_reference_test {
    ($name:ident, $command:ident, $reference:expr) => {
        #[test]
        #[ignore = "red until #746: member references must reach the project run; implement removes this"]
        fn $name() {
            let run = ProjectRun::new();
            // Positive controls pin the live run and the unchanged reference tiers.
            run.attach(Some("."));
            run.attach(Some("index.panta.md"));
            run.attach(Some(&run.id));
            run.attach(Some(&run.id[..3]));
            run.$command($reference);
        }
    };
}

member_reference_test!(member_run_746_stop_directory, stop, Some("streams"));
member_reference_test!(member_run_746_stop_index, stop, Some("streams/index.rhei.md"));
member_reference_test!(member_run_746_stop_omitted, stop, None);
member_reference_test!(member_run_746_attach_directory, attach, Some("streams"));
member_reference_test!(member_run_746_attach_index, attach, Some("streams/index.rhei.md"));
member_reference_test!(member_run_746_attach_omitted, attach, None);
