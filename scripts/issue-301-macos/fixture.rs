//! Retain the actual E2E snapshot after its original child stop (§FS-rhei-validate.5).
//! Only the named test imports this module in the disposable source copy.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Measurements survive both assertion failure and fixture cleanup (§FS-rhei-migrate.5).
pub(super) struct Evidence {
    pub(super) output: PathBuf,
    plan: PathBuf,
    stderr: PathBuf,
    metadata: serde_json::Value,
    saved: bool,
}

impl Evidence {
    /// All setup I/O precedes spawn; no measurement I/O extends polling (§FS-rhei-validate.5).
    pub(super) fn new(plan: &Path, stderr: &Path) -> Self {
        let started = Instant::now();
        let output = PathBuf::from(std::env::var_os("ISSUE_301_CASE_OUTPUT").expect("case output"))
            .join(format!("case-{}", std::process::id()));
        fs::create_dir(&output).expect("unique measurement directory");
        let directory = plan.parent().expect("fixture directory");
        for name in ["plan.rhei.md", "states.yaml"] {
            fs::copy(directory.join(name), output.join(format!("{name}.before")))
                .expect("retain authored input before watch");
        }
        let metadata = serde_json::json!({
            "harness_pid": std::process::id(), "cwd": directory,
            "cwd_canonical": fs::canonicalize(directory).ok(),
            "plan": plan, "discovered_target": super::discovered_target(plan),
            "stderr": stderr, "stderr_canonical": fs::canonicalize(stderr).ok(),
            "output": output, "output_canonical": fs::canonicalize(&output).ok(),
            "native_temp_dir": std::env::temp_dir(), "tmpdir": std::env::var("TMPDIR").ok(),
            "home": directory.join(".home"), "xdg_state_home": directory.join(".home/state"),
            "stdout": "null (unchanged)", "args": ["validate", "--watch"],
            "poll_ms": 25, "deadline_seconds": 10, "extra_observation_seconds": 0,
            "before_spawn_probe_ns": started.elapsed().as_nanos().to_string(),
            "snapshot_available": false, "normal_stop_completed": false,
        });
        fs::write(output.join("initial.json"), metadata.to_string()).expect("initial measurement");
        Self {
            output,
            plan: plan.to_path_buf(),
            stderr: stderr.to_path_buf(),
            metadata,
            saved: false,
        }
    }

    /// In-memory assignment only between spawn and the unchanged deadline (§FS-rhei-validate.5).
    pub(super) fn child(&mut self, pid: u32) {
        self.metadata["child_pid"] = pid.into();
    }

    /// Save the very String the assertion reads, after kill/wait (§FS-rhei-errors.1.2).
    pub(super) fn finish(&mut self, rendered: &str) {
        self.metadata["normal_stop_completed"] = true.into();
        match fs::write(self.output.join("poll-snapshot.stderr"), rendered.as_bytes()) {
            Ok(()) => self.metadata["snapshot_available"] = true.into(),
            Err(error) => self.metadata["snapshot_error"] = error.to_string().into(),
        }
        self.save();
    }

    /// Retain final bytes without panicking over the original test (§FS-rhei-migrate.5).
    fn save(&mut self) {
        let started = Instant::now();
        let directory = self.plan.parent().expect("fixture directory");
        let mut errors = Vec::new();
        for (from, to) in [
            (self.stderr.clone(), "stderr.txt"),
            (directory.join("plan.rhei.md"), "plan.rhei.md.after"),
            (directory.join("states.yaml"), "states.yaml.after"),
        ] {
            if let Err(error) = fs::copy(&from, self.output.join(to)) {
                errors.push(format!("{}: {error}", from.display()));
            }
        }
        self.metadata["retention_errors"] = serde_json::json!(errors);
        self.metadata["after_stop_copy_ns"] = started.elapsed().as_nanos().to_string().into();
        self.metadata["retention_during_unwind"] = std::thread::panicking().into();
        self.saved = fs::write(self.output.join("case.json"), self.metadata.to_string()).is_ok();
    }
}

/// Declared before ChildGuard, so unwinding reaps the watcher first (§FS-rhei-validate.5).
impl Drop for Evidence {
    fn drop(&mut self) {
        if !self.saved {
            self.save();
        }
    }
}
