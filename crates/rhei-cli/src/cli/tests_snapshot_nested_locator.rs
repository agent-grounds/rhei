// agent-grounds/rhei#146: the optional `FlatById` locator keys — nested search,
// trailing-UUID session ids, and `cwd`-confirmed candidates — plus the
// observed-target header scan they depend on.

// Split from tests_snapshot_runtime_fixed_location.rs to stay under the
// file-size budget. §AR-source-file-size.3

/// A rollout laid out the way codex writes one: a `session_meta` first record
/// carrying the working directory and the provider, filler records, and a
/// `turn_context` carrying the model at `model_line`. `model_line` of `0`
/// leaves the `turn_context` out entirely.
// §FS-rhei-snapshots.10.2.1
fn nested_rollout_jsonl(cwd: &str, provider: &str, model: &str, model_line: usize) -> String {
    let mut lines = vec![serde_json::json!({
        "type": "session_meta",
        "payload": {"session_id": "ignored", "cwd": cwd, "model_provider": provider}
    })
    .to_string()];
    let filler =
        serde_json::json!({"type": "response_item", "payload": {"role": "assistant"}}).to_string();
    if model_line > 0 {
        while lines.len() + 1 < model_line {
            lines.push(filler.clone());
        }
        lines.push(
            serde_json::json!({"type": "turn_context", "payload": {"cwd": cwd, "model": model}})
                .to_string(),
        );
    } else {
        for _ in 0..4 {
            lines.push(filler.clone());
        }
    }
    format!("{}\n", lines.join("\n"))
}

/// Write `body` as `<root>/<day>/rollout-<stamp>-<uuid>.jsonl`, stamped at
/// `mtime`, and return the path. `day` is the `<year>/<month>/<day>` tail the
/// agent partitions by, which is exactly what a flat `read_dir` cannot see.
// §FS-rhei-snapshots.9.1.1
fn write_nested_rollout(
    root: &Path,
    day: &str,
    stamp: &str,
    uuid: &str,
    body: &str,
    mtime: std::time::SystemTime,
) -> PathBuf {
    let dir = day.split('/').fold(root.to_path_buf(), |acc, part| acc.join(part));
    fs::create_dir_all(&dir).expect("rollout day dir");
    let path = dir.join(format!("rollout-{stamp}-{uuid}.jsonl"));
    fs::write(&path, body).expect("write rollout");
    set_file_mtime(&path, mtime);
    path
}

/// The spawn working directory as the child process observes it, which is what
/// `confirm_cwd_path` compares a candidate's header against.
// §FS-rhei-snapshots.9.1.1
fn canonical_spawn_cwd(dir: &Path) -> String {
    rhei_core::platform::canonical_path(dir).expect("canonical spawn dir").display().to_string()
}

/// A profile shaped the way codex's is: one file per session under a fixed
/// nested root it derives itself, no `session_dir_flag`, no `assign_id_flag`.
// §FS-rhei-snapshots.9.1.1 §FS-rhei-snapshots.9.3.4
fn nested_locator_settings(dir_template: &str) -> RheiSettings {
    let session = serde_json::json!({
        "resume": {"flag": "resume"},
        "layout": {
            "kind": "FlatById",
            "dir_template": dir_template,
            "ext": "jsonl",
            "nested": true,
            "id_from_stem": "trailing_uuid",
            "confirm_cwd_path": ["payload", "cwd"]
        }
    });
    let mut agents = BTreeMap::new();
    agents.insert(
        "nested".to_string(),
        CustomAgentProfile {
            command: vec!["nested".to_string()],
            stdin_prompt: true,
            session: Some(session),
            ..Default::default()
        },
    );
    RheiSettings { agents, ..Default::default() }
}

fn write_nested_emit_machine(dir: &Path) {
    fs::write(
        dir.join("states.yaml"),
        r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: nested:openai:model
    snapshot:
      emit:
        name: impl
        on: always
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
    )
    .expect("write states");
}

/// Stand up the nested-locator workspace, run preload, and hand back what emit
/// needs: the machine, the task, the resolved agent, the preload, the resolved
/// sessions root, and the scan floor the candidates are stamped against.
struct NestedEmitFixture {
    dir: SnapshotWorkspace,
    settings: RheiSettings,
    loaded: LoadedPlan,
    machine: rhei_validator::StateMachine,
    resolved: ResolvedAgent,
    preload: SnapshotPreload,
    sessions_root: PathBuf,
    floor: std::time::SystemTime,
}

fn nested_emit_fixture() -> NestedEmitFixture {
    let dir = snapshot_workspace();
    write_nested_emit_machine(dir.path());
    let sessions_root = dir.path().join("agent-sessions");
    fs::create_dir_all(&sessions_root).expect("sessions root");
    let settings = nested_locator_settings(sessions_root.to_str().expect("utf8 path"));
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let preload = {
        let task = loaded.rhei.tasks.first().expect("task");
        preload_snapshot_inherit_before_spawn(
            dir.path(),
            single_root_preload(dir.path()),
            dir.path(),
            &machine,
            task,
            "pending",
            &resolved,
            &settings,
            1,
            None,
            &default_run_options(),
        )
        .expect("nested fixed-location profile preloads")
    };
    let floor = preload.fixed_session_scan_floor.expect("scan floor recorded");
    NestedEmitFixture { dir, settings, loaded, machine, resolved, preload, sessions_root, floor }
}

impl NestedEmitFixture {
    fn cwd(&self) -> String {
        canonical_spawn_cwd(self.dir.path())
    }

    /// Run emit and return the named snapshot the state declared.
    fn emit(&self) -> SnapshotRecord {
        let task = self.loaded.rhei.tasks.first().expect("task");
        let log_path = self.dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "log\n").expect("log");
        emit_snapshots_after_agent_exit(
            self.dir.path(),
            &self.machine,
            &self.settings,
            task,
            "pending",
            Some("done"),
            &self.resolved,
            &log_path,
            1,
            SnapshotCompletion::Success,
            &self.preload,
        )
        .expect("emit locates the nested transcript");
        read_snapshot_records(&snapshot_cache_dir(&self.settings, self.dir.path()))
            .expect("records")
            .into_iter()
            .find(|record| record.snapshot_name == "impl")
            .expect("named snapshot written")
    }
}

const REAL_UUID: &str = "01a059e8-64e0-78c3-8110-e683296f50a2";
const DECOY_UUID: &str = "01a04a76-4058-7a82-8ede-a2c0b9c7e527";

/// agent-grounds/rhei#146 acceptance 2: the scan descends below `dir_template`, and
/// the session id recorded is the bare UUID out of the `rollout-<stamp>-<uuid>`
/// stem — the value `resume` takes back, not the file's name.
/// §FS-rhei-snapshots.9.1.1 §FS-rhei-snapshots.10.2
#[test]
fn nested_locator_finds_a_dated_rollout_and_records_its_trailing_uuid() {
    let fixture = nested_emit_fixture();
    let body = nested_rollout_jsonl(&fixture.cwd(), "openai", "gpt-5.6-luna", 8);
    write_nested_rollout(
        &fixture.sessions_root,
        "2026/09/01",
        "2026-09-01T00-19-57",
        REAL_UUID,
        &body,
        fixture.floor + std::time::Duration::from_secs(1),
    );

    let named = fixture.emit();
    assert_eq!(
        named.manifest.get("session_id").and_then(serde_json::Value::as_str),
        Some(REAL_UUID),
        "the manifest must carry the bare uuid, not the rollout stem"
    );
    assert_eq!(fs::read_to_string(named.transcript_path()).expect("transcript"), body);
}

/// agent-grounds/rhei#146 acceptance 3: a shared session root holds other projects'
/// runs. The decoy here wins on every axis the scan ranks by — newer mtime, a
/// later date directory, a bigger file — and loses only on the `cwd` its header
/// names, which is the one that decides. §FS-rhei-snapshots.9.1.1
#[test]
fn cwd_confirmation_rejects_a_newer_rollout_from_another_working_directory() {
    let fixture = nested_emit_fixture();
    let mine = nested_rollout_jsonl(&fixture.cwd(), "openai", "gpt-5.6-luna", 8);
    write_nested_rollout(
        &fixture.sessions_root,
        "2026/09/01",
        "2026-09-01T00-19-57",
        REAL_UUID,
        &mine,
        fixture.floor + std::time::Duration::from_secs(1),
    );
    let theirs = nested_rollout_jsonl("/somewhere/else/entirely", "openai", "gpt-5.6-sol", 24);
    assert!(theirs.len() > mine.len(), "the decoy must also be the bigger file");
    write_nested_rollout(
        &fixture.sessions_root,
        "2026/09/02",
        "2026-09-02T11-02-13",
        DECOY_UUID,
        &theirs,
        fixture.floor + std::time::Duration::from_secs(30),
    );

    let named = fixture.emit();
    assert_eq!(
        named.manifest.get("session_id").and_then(serde_json::Value::as_str),
        Some(REAL_UUID),
        "the newest rollout belongs to another working directory and must be skipped"
    );
    assert_eq!(fs::read_to_string(named.transcript_path()).expect("transcript"), mine);
}

/// agent-grounds/rhei#146 acceptance 3: a header that cannot be read is a rejection,
/// not a silent accept — neither an unparsable first record nor a well-formed
/// one with nothing at `confirm_cwd_path` may be taken for this spawn's
/// transcript. §FS-rhei-snapshots.9.1.1
#[test]
fn cwd_confirmation_rejects_candidates_whose_header_cannot_be_read() {
    let fixture = nested_emit_fixture();
    let mine = nested_rollout_jsonl(&fixture.cwd(), "openai", "gpt-5.6-luna", 8);
    write_nested_rollout(
        &fixture.sessions_root,
        "2026/09/01",
        "2026-09-01T00-19-57",
        REAL_UUID,
        &mine,
        fixture.floor + std::time::Duration::from_secs(1),
    );
    write_nested_rollout(
        &fixture.sessions_root,
        "2026/09/02",
        "2026-09-02T09-00-00",
        "01a04a76-4058-7a82-8ede-a2c0b9c7e528",
        "this is not json at all, and never was\n",
        fixture.floor + std::time::Duration::from_secs(20),
    );
    write_nested_rollout(
        &fixture.sessions_root,
        "2026/09/03",
        "2026-09-03T09-00-00",
        DECOY_UUID,
        "{\"type\":\"session_meta\",\"payload\":{\"session_id\":\"x\"}}\n",
        fixture.floor + std::time::Duration::from_secs(40),
    );

    let named = fixture.emit();
    assert_eq!(
        named.manifest.get("session_id").and_then(serde_json::Value::as_str),
        Some(REAL_UUID),
        "an unconfirmable header must be rejected rather than accepted"
    );
    assert_eq!(fs::read_to_string(named.transcript_path()).expect("transcript"), mine);
}

/// agent-grounds/rhei#146 acceptance 4: the model is not in the session header — it
/// arrives with the first turn, past the eight-line window pi needed. The scan
/// takes the provider and the model independently, from wherever in the
/// documented window each first appears. §FS-rhei-snapshots.10.2.1
#[test]
fn observed_target_reads_the_model_from_a_turn_context_past_the_pi_window() {
    let fixture = nested_emit_fixture();
    let body = nested_rollout_jsonl(&fixture.cwd(), "openai", "gpt-5.6-luna", 20);
    assert_eq!(body.lines().count(), 20, "the model record must sit past the pi window");
    write_nested_rollout(
        &fixture.sessions_root,
        "2026/09/01",
        "2026-09-01T00-19-57",
        REAL_UUID,
        &body,
        fixture.floor + std::time::Duration::from_secs(1),
    );

    let named = fixture.emit();
    assert_eq!(
        named.manifest.get("observed_model").and_then(serde_json::Value::as_str),
        Some("gpt-5.6-luna"),
        "observed_model must come from the transcript, not from the declared target"
    );
    assert_eq!(
        named.manifest.get("observed_provider").and_then(serde_json::Value::as_str),
        Some("openai")
    );
    assert_eq!(
        named.manifest.get("declared_model").and_then(serde_json::Value::as_str),
        Some("model"),
        "the declared target is what the fallback would have produced"
    );
}

/// A regression guard rather than a pin: today every non-pi transcript falls
/// back to the declared target, so this already passes. It holds the documented
/// fallback in place once the scan becomes general.
// §FS-rhei-snapshots.10.2.1
#[test]
fn observed_target_falls_back_to_declared_when_the_window_holds_no_model() {
    let fixture = nested_emit_fixture();
    let body = nested_rollout_jsonl(&fixture.cwd(), "openai", "gpt-5.6-luna", 40);
    assert_eq!(body.lines().count(), 40, "the model record must sit past the documented window");
    write_nested_rollout(
        &fixture.sessions_root,
        "2026/09/01",
        "2026-09-01T00-19-57",
        REAL_UUID,
        &body,
        fixture.floor + std::time::Duration::from_secs(1),
    );

    let named = fixture.emit();
    assert_eq!(
        named.manifest.get("observed_model").and_then(serde_json::Value::as_str),
        Some("model"),
        "a model past the window falls back to the declared one"
    );
}

// agent-grounds/rhei#146 acceptance 1 and 9: the built-in codex profile is the §FS-rhei-snapshots.9.2
// table row, in code. §FS-rhei-snapshots.9.2 §FS-rhei-snapshots.9.3.4
#[test]
fn built_in_codex_profile_declares_the_nested_session_locator() {
    let profile = built_in_agents().remove("codex").expect("codex");
    let session = profile.session.as_ref().expect("codex declares a session block");
    assert_eq!(snapshot_strategy_flag(session, "resume").as_deref(), Some("resume"));
    assert!(
        snapshot_strategy_flag(session, "fork").is_none(),
        "codex's `fork` takes a session id, which `ForkStrategy::Native` cannot express: it \
         hands the agent the source snapshot's transcript path, and the preload prefers fork \
         over resume whenever both are declared, so declaring it would defeat the resume"
    );
    assert_eq!(
        snapshot_session_string(session, "no_session_flag").as_deref(),
        Some("--ephemeral")
    );
    assert!(
        snapshot_session_string(session, "session_dir_flag").is_none(),
        "codex has no session_dir_flag analogue"
    );
    assert!(
        snapshot_session_string(session, "assign_id_flag").is_none(),
        "codex names its own sessions"
    );
    assert_eq!(
        session.get("interactive").and_then(|value| value.get("command")),
        Some(&serde_json::json!(["codex"])),
        "interactive continuation is the top-level `codex` command, not `codex exec`"
    );
    let layout = snapshot_session_layout(session).expect("layout");
    assert_eq!(snapshot_layout_kind(layout).as_deref(), Some("FlatById"));
    assert_eq!(snapshot_layout_ext(layout).as_deref(), Some("jsonl"));
    assert_eq!(snapshot_layout_dir_template(layout).as_deref(), Some("~/.codex/sessions"));
    assert_eq!(layout.get("nested"), Some(&serde_json::Value::Bool(true)));
    assert_eq!(
        layout.get("id_from_stem").and_then(serde_json::Value::as_str),
        Some("trailing_uuid")
    );
    assert_eq!(layout.get("confirm_cwd_path"), Some(&serde_json::json!(["payload", "cwd"])));
}
