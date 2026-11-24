//! A self-contained port of #774's three-member shared-cache fixture.
//! §FS-rhei-validate.4.7

use std::fs;
use std::path::{Path, PathBuf};

use super::help_target_tests::Case;
use super::validation_override_support::member;
use super::*;

pub(super) const TARGET: &str = "codex[yolo]:openai:gpt-6-astra";
pub(super) const SLUG: &str = "codex-yolo-openai-gpt-6-astra";

pub(super) struct Fixture {
    pub case: Case,
    pub project: PathBuf,
    pub live: Vec<String>,
    pub own: String,
    pub others: Vec<String>,
}

impl Fixture {
    pub fn new(prefix: &str) -> Self {
        let case = Case::new(prefix);
        // Spaces make the outside-cwd inspection hint prove its quoting too.
        let project = case.root.join("shared project");
        fs::create_dir_all(&project).unwrap();
        write_fixture_file(&project, "index.panta.md", "# Panta: Snapshot streams\n");
        let mut live = Vec::new();
        for (id, task, state) in [
            ("csa-models", "train", "model"),
            ("csa-fix", "deliver", "deliver"),
            ("csa-profile", "measure", "measure"),
        ] {
            let root = member(&project, id, &[(task, state)]);
            write_fixture_file(
                &root,
                "states.yaml",
                &format!(
                    "name: {id}\nversion: 1\nstates:\n  {state}:\n    initial: true\n    \
                     description: Work\n    target: {TARGET}\n    agent_timeout: 60s\n  done:\n    final: true\n    \
                     description: Done\ntransitions:\n  - {{ from: {state}, to: done }}\n"
                ),
            );
            live.push(record(&project, &format!("{id}.{task}"), state, 1, SLUG, "orchestrator"));
        }
        let own = record(&project, "csa-models.retired", "model", 1, SLUG, "orchestrator");
        let mut others = Vec::new();
        // A renamed state, two visits, then a deleted ticket.
        for visit in [1, 2] {
            others.push(record(&project, "csa-fix.deliver", "review", visit, SLUG, "orchestrator"));
        }
        others.push(record(&project, "csa-fix.hotfix", "deliver", 1, SLUG, "orchestrator"));
        // A deleted ticket, two visits, then a dropped target from operator capture.
        for visit in [1, 2] {
            others.push(record(
                &project,
                "csa-profile.baseline",
                "measure",
                visit,
                SLUG,
                "orchestrator",
            ));
        }
        others.push(record(
            &project,
            "csa-profile.measure",
            "measure",
            1,
            "codex-yolo-openai-retired-model",
            "operator",
        ));
        Self { case, project, live, own, others }
    }

    pub fn validate(&self, cwd: &Path, target: &[&str]) -> String {
        let mut args = vec!["validate"];
        args.extend_from_slice(target);
        let out = self.case.rhei(cwd, &args);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(0), "{said}\n{}", stderr(&out));
        assert!(said.contains("Validation succeeded"), "{said}");
        for live in &self.live {
            assert!(!said.contains(live), "live snapshot reported as orphaned: {live}\n{said}");
        }
        said
    }

    pub fn orphan_rows(&self, all_origins: bool) -> Vec<serde_json::Value> {
        let mut args = vec!["snapshot", "list", "--orphaned", "--format", "json"];
        if all_origins {
            args.extend_from_slice(&["--produced-by", "all"]);
        }
        let out = self.case.rhei(&self.project, &args);
        assert!(out.status.success(), "{}", stderr(&out));
        serde_json::from_str(&stdout(&out)).expect("readable snapshot manifests")
    }
}

fn record(project: &Path, task: &str, state: &str, visit: u64, slug: &str, origin: &str) -> String {
    let dir = project
        .join(".rhei/cache/snapshots")
        .join(task)
        .join("_state")
        .join(state)
        .join(visit.to_string())
        .join(slug)
        .join("g1");
    fs::create_dir_all(&dir).unwrap();
    let manifest = serde_json::json!({
        "version": 1, "rhei_version": "0.0.0", "snapshot_name": "_state",
        "task_id": task, "emitting_state": state, "visit": visit, "generation": 1,
        "created_at": "2020-01-01T00:00:00Z", "completion": "success",
        "produced_by": origin, "declared_provider": "openai", "declared_model": "gpt-6-astra",
        "observed_provider": "openai", "observed_model": "gpt-6-astra", "session_id": "s",
        "transcript_path": "transcript.jsonl", "transcript_sha256": "0", "transcript_bytes": 0,
        "target": { "selector": TARGET, "slug": slug, "resolved": {} },
        "session_layout": { "kind": "FlatById", "ext": "jsonl" }, "parent_ref": null,
    });
    fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    fs::write(dir.join("transcript.jsonl"), "").unwrap();
    format!("{task}:_state:{state}@{visit}:{slug}/g1")
}

#[track_caller]
pub(super) fn assert_own_detail(said: &str, own: &str) {
    assert_eq!(
        said.lines()
            .filter(|line| line.starts_with("warning: snapshot ") && line.contains(own))
            .count(),
        1,
        "the named member's deleted ticket must retain one detailed warning:\n{said}"
    );
}

#[track_caller]
pub(super) fn summary(said: &str) -> &str {
    let other_lines: Vec<_> = said
        .lines()
        .filter(|line| {
            line.starts_with("warning:")
                && (line.contains("csa-fix") || line.contains("csa-profile"))
        })
        .collect();
    assert_eq!(
        other_lines.len(),
        1,
        "other members' six orphan records must occupy one combined warning:\n{said}"
    );
    let line = other_lines[0];
    assert!(
        line.split_whitespace().any(|word| word.parse::<usize>() == Ok(6))
            && line.contains("orphan"),
        "missing total:\n{line}"
    );
    let fix = line.find("csa-fix: 3").expect("three csa-fix orphans, including a deleted ticket");
    let profile =
        line.find("csa-profile: 3").expect("three csa-profile orphans across both origins");
    assert!(fix < profile, "owner counts must be sorted by rhei id: {line}");
    assert!(line.contains("rhei snapshot list"), "missing read-only inspection hint: {line}");
    assert!(line.contains("--orphaned") && line.contains("--produced-by all"), "{line}");
    line
}
