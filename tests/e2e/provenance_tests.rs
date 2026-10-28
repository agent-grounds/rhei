//! What a run says about where it came from: the build, the template source,
//! the checkout HEAD at start, and the checkpointed provenance line of
//! `rhei summary`. §FS-rhei-version.2 §FS-rhei-library.7.1
//! §FS-rhei-templates.6.1.5 §FS-rhei-run-headless.2 §FS-rhei-run-json.2.1
//! §FS-rhei-cost-accounting.3.5 §FS-rhei-summary.2.4 §FS-rhei-summary.6

use std::fs;

use super::accounting_support::{invocation_records, last_run_id};
use super::accounting_unrecorded_spawn_support::{
    spawn_workspace, NOFAMILY_MACHINE, NOFAMILY_PLAN,
};
use super::headless_support::parse_records;
use super::provenance_support::{
    assert_fence_shape, build_as_printed, checkpoint_id, commit_repository, laid_fence, rhei_in,
    run, source_line, Lay, TEMPLATE_REMOTE, WORKSPACE_REMOTE,
};
use super::{assert_success, rhei_command, unique_temp_dir, CliRun};

#[test]
fn rhei_version_prints_the_build_line_after_the_four_component_lines() {
    // §FS-rhei-version.2 §FS-rhei-version.3: a fifth `<component> <version>`
    // line, the first four untouched.
    let dir = unique_temp_dir("provenance-version");
    let result = run(rhei_in(&dir, &dir), &["version"]);
    assert_success(&result);

    let lines: Vec<&str> = result.stdout.lines().collect();
    let components: Vec<&str> =
        lines.iter().map(|line| line.split(' ').next().unwrap_or("")).collect();
    assert_eq!(
        components,
        ["rhei-cli", "rhei-core", "rhei-validator", "rhei-output", "rhei-build"],
        "got:\n{}",
        result.stdout
    );
    let build = lines[4].strip_prefix("rhei-build ").expect("build line");
    let commit = build.strip_suffix("-dirty").unwrap_or(build);
    assert!(
        build == "unknown"
            || (commit.len() == 40
                && commit.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))),
        "the build is `<40-hex>[-dirty]` or `unknown`; got {build:?}"
    );
}

#[test]
fn rhei_dash_dash_version_carries_the_build_in_a_parenthetical() {
    // §FS-rhei-version.2: `rhei <version> (<sha7>[, dirty])` or `(build unknown)`,
    // the version still the second field.
    let dir = unique_temp_dir("provenance-dash-version");
    let result = run(rhei_in(&dir, &dir), &["--version"]);
    assert_success(&result);

    let line = result.stdout.trim_end();
    let fields: Vec<&str> = line.splitn(3, ' ').collect();
    assert_eq!(fields[0], "rhei", "got {line:?}");
    assert_eq!(
        fields[1],
        env!("CARGO_PKG_VERSION"),
        "the version stays the second field: {line:?}"
    );
    let paren = fields.get(2).copied().unwrap_or("");
    let inner = paren.strip_prefix('(').and_then(|rest| rest.strip_suffix(')'));
    let ok = inner.is_some_and(|inner| {
        let commit = inner.strip_suffix(", dirty").unwrap_or(inner);
        inner == "build unknown"
            || (commit.len() == 7 && commit.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    });
    assert!(ok, "expected `(<sha7>)`, `(<sha7>, dirty)` or `(build unknown)`; got {line:?}");
}

#[test]
fn a_committed_template_lays_a_clean_fence_naming_its_commit() {
    // §FS-rhei-library.7.1 §FS-rhei-templates.6.1.5: the fence names the
    // reduced remote and the commit, and the template is `clean`.
    let lay = Lay::new("provenance-clean", Some(TEMPLATE_REMOTE));
    let result = lay.instantiate(&lay.template, &[]);
    assert_success(&result);

    let fence = laid_fence(&lay.laid_machine());
    let git = assert_fence_shape(&fence, "clean");
    assert_eq!(git, Some(format!("git:example-org/templates@{}", lay.template_commit)));
    assert_eq!(source_line(&result.stdout), "Source: template clean · inputs none");
}

#[test]
fn an_edited_template_lays_dirty_and_the_source_line_names_its_root() {
    // §FS-rhei-templates.6.1.5 §FS-rhei-templates.6.1.3: recorded, not
    // refused; `Source:` names the one non-clean root and its file count.
    let lay = Lay::new("provenance-dirty", None);
    let machine = lay.template.join("states.yaml");
    let edited = fs::read_to_string(&machine).expect("template machine") + "# edited\n";
    fs::write(&machine, edited).expect("edit the template");

    let result = lay.instantiate(&lay.template, &[]);
    assert_success(&result);

    let git = assert_fence_shape(&laid_fence(&lay.laid_machine()), "dirty");
    assert_eq!(git, Some(format!("git:{}", lay.template_commit)), "no remote: the bare commit");
    assert_eq!(source_line(&result.stdout), "Source: template tpl dirty (1 file) · inputs none");
}

#[test]
fn a_values_file_outside_any_repository_leaves_the_fence_clean() {
    // §FS-rhei-templates.6.1.5: inputs never enter the state word; they show
    // on `Source:` only.
    let lay = Lay::new("provenance-values", Some(TEMPLATE_REMOTE));
    let values = lay.dir.join("vals");
    fs::create_dir_all(&values).expect("values dir");
    fs::write(values.join("values.yaml"), "{}\n").expect("values file");

    let result = lay.instantiate(&lay.template, &["--values", "vals/values.yaml"]);
    assert_success(&result);

    assert_fence_shape(&laid_fence(&lay.laid_machine()), "clean");
    assert_eq!(
        source_line(&result.stdout),
        "Source: template clean · inputs vals unversioned (1 file)"
    );
}

#[cfg(unix)]
#[test]
fn a_template_reached_through_a_symlink_names_the_target_repository() {
    // §FS-rhei-templates.6.1.5: symlinks are resolved before the repository
    // is asked, so the link's own directory, in no repository, does not count.
    let lay = Lay::new("provenance-symlink", Some(TEMPLATE_REMOTE));
    let links = lay.dir.join("links");
    fs::create_dir_all(&links).expect("links dir");
    std::os::unix::fs::symlink(&lay.template, links.join("tiny")).expect("symlink the template");

    let result = lay.instantiate(&links.join("tiny"), &[]);
    assert_success(&result);

    let git = assert_fence_shape(&laid_fence(&lay.laid_machine()), "clean");
    assert_eq!(git, Some(format!("git:example-org/templates@{}", lay.template_commit)));
}

#[test]
fn with_no_git_on_path_the_lay_reads_unknown_and_still_succeeds() {
    // §FS-rhei-templates.6.1.5 §REQ-cross-platform: no git is `unknown`, no
    // `git:` field, and the lay is never refused for it.
    let lay = Lay::new("provenance-no-git", Some(TEMPLATE_REMOTE));
    let empty = lay.dir.join("empty-path");
    fs::create_dir_all(&empty).expect("empty PATH dir");
    let mut cmd = rhei_in(&lay.dir, &lay.dir);
    cmd.env("PATH", &empty);
    let template = lay.template.display().to_string();
    let result = run(cmd, &["instantiate", &template, "--output", "ws"]);
    assert_success(&result);

    let git = assert_fence_shape(&laid_fence(&lay.laid_machine()), "unknown");
    assert_eq!(git, None, "no git, no commit");
    assert!(source_line(&result.stdout).contains("unknown"), "got:\n{}", result.stdout);
}

/// A laid, committed workspace that has been run to the end: `t1` (agent),
/// then `t2` (the program that took `out/checkpoint.json` and `out/live.md`),
/// then `t3` (agent).
struct Ran {
    lay: Lay,
    head: String,
}

fn ran(prefix: &str, workspace_remote: &str) -> Ran {
    let lay = Lay::new(prefix, Some(TEMPLATE_REMOTE));
    assert_success(&lay.instantiate(&lay.template, &[]));
    let ws = lay.workspace();
    fs::write(ws.join(".gitignore"), "runtime/\n").expect("ignore runtime");
    let head = commit_repository(&lay.dir, &ws, Some(workspace_remote));
    let result = run(rhei_in(&lay.dir, &ws), &["run", ".", "--no-tui", "--no-callbacks"]);
    assert_success(&result);
    Ran { lay, head }
}

impl Ran {
    fn summary(&self, args: &[&str]) -> CliRun {
        let ws = self.lay.workspace().display().to_string();
        let mut all = vec!["summary", ws.as_str()];
        all.extend_from_slice(args);
        run(rhei_in(&self.lay.dir, &self.lay.dir), &all)
    }

    fn snapshot(&self, name: &str) -> Vec<u8> {
        let path = self.lay.out.join(name);
        let err = self.lay.out.join(format!("{name}.stderr"));
        let stderr = if err.is_file() { super::stderr_from_file(&err) } else { String::new() };
        let bytes = fs::read(&path).unwrap_or_default();
        assert!(!bytes.is_empty(), "the program step wrote no {name}; its stderr:\n{stderr}");
        bytes
    }

    fn checkpoint(&self) -> serde_json::Value {
        let bytes = self.snapshot("checkpoint.json");
        serde_json::from_slice(&bytes).unwrap_or_else(|err| {
            panic!(
                "--print-checkpoint printed no JSON ({err}):\n{}",
                String::from_utf8_lossy(&bytes)
            )
        })
    }
}

fn assert_run_fields(value: &serde_json::Value, head: &str, what: &str) {
    let build = &value["rhei_build"];
    assert_eq!(build["version"], env!("CARGO_PKG_VERSION"), "{what}: rhei_build in {value:#}");
    assert!(build["commit"].is_string() || build["commit"].is_null(), "{what}: {value:#}");
    assert!(build["dirty"].is_boolean() || build["dirty"].is_null(), "{what}: {value:#}");
    assert_eq!(
        value["checkout_head_at_start"],
        serde_json::json!({ "repo": "example-org/example-repo", "commit": head }),
        "{what}: checkout_head_at_start in {value:#}"
    );
}

#[test]
fn a_run_records_its_build_and_the_checkout_head_at_start() {
    // §FS-rhei-run-headless.2 §FS-rhei-run-json.2.1 §FS-rhei-cost-accounting.3.5:
    // both fields on the descriptor, the stream's head and every record.
    let ran = ran("provenance-run", WORKSPACE_REMOTE);
    let ws = ran.lay.workspace();

    let descriptor: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(ws.join("runtime/run.json")).expect("run.json"))
            .expect("run.json parses");
    assert_run_fields(&descriptor, &ran.head, "run.json");

    let events = fs::read_to_string(ws.join("runtime/events.jsonl")).expect("events");
    let started = parse_records(&events)
        .into_iter()
        .find(|record| record["event"] == "run_started")
        .expect("a run_started record");
    assert_eq!(started["schema"], 1, "the additions keep schema 1");
    assert_run_fields(&started, &ran.head, "run_started");

    let records = invocation_records(&ws);
    assert_eq!(records.len(), 2, "fixture: t1 and t3 each wrote a record");
    for record in &records {
        assert_run_fields(record, &ran.head, "invocation record");
    }
}

#[test]
fn a_run_outside_any_repository_records_a_null_head() {
    // §FS-rhei-run-headless.2: always present, `null` when nothing is known.
    let lay = Lay::new("provenance-no-repo", None);
    assert_success(&lay.instantiate(&lay.template, &[]));
    let ws = lay.workspace();
    assert_success(&run(rhei_in(&lay.dir, &ws), &["run", ".", "--no-tui", "--no-callbacks"]));

    let descriptor: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(ws.join("runtime/run.json")).expect("run.json"))
            .expect("run.json parses");
    let object = descriptor.as_object().expect("descriptor is an object");
    assert!(object.contains_key("rhei_build"), "always present: {descriptor:#}");
    assert_eq!(object.get("checkout_head_at_start"), Some(&serde_json::Value::Null));
}

#[test]
fn the_provenance_line_is_exact_and_prints_last() {
    // §FS-rhei-summary.2.4 §FS-rhei-summary.3: the fourth part, after a blank
    // line, and inside `<details>` just before it closes.
    let ran = ran("provenance-line", WORKSPACE_REMOTE);
    let ws = ran.lay.workspace();
    let run_id = last_run_id(&ws);
    let print = ran.summary(&["--print-checkpoint"]);
    assert_success(&print);

    let expected = format!(
        "Provenance: rhei {version} @ {build} · checkout HEAD at start \
         example-org/example-repo@{head7} · template `tiny` 1.0.0 @ \
         example-org/templates@{tpl7} (clean) · checkpoint {id}: run {run6}, 2 invocations",
        version = env!("CARGO_PKG_VERSION"),
        build = build_as_printed(&ran.lay.dir),
        head7 = &ran.head[..7],
        tpl7 = &ran.lay.template_commit[..7],
        id = checkpoint_id(print.stdout.as_bytes()),
        run6 = &run_id[..6],
    );

    let plain = ran.summary(&[]);
    assert_success(&plain);
    let lines: Vec<&str> = plain.stdout.lines().collect();
    assert_eq!(lines.last().copied(), Some(expected.as_str()), "got:\n{}", plain.stdout);
    assert_eq!(lines[lines.len() - 2], "", "a blank line before it; got:\n{}", plain.stdout);

    let details = ran.summary(&["--details"]);
    assert_success(&details);
    let lines: Vec<&str> = details.stdout.lines().collect();
    assert_eq!(&lines[lines.len() - 2..], [expected.as_str(), "</details>"], "{}", details.stdout);
}

#[test]
fn a_checkpoint_replays_byte_for_byte_what_the_live_summary_printed() {
    // §FS-rhei-summary.1 §FS-rhei-summary.6: taken beside a live `--details` mid-run, the
    // checkpoint replays that text exactly after `t3` has run, tally included.
    let ran = ran("provenance-replay", WORKSPACE_REMOTE);
    let checkpoint = ran.checkpoint();
    let live = String::from_utf8(ran.snapshot("live.md")).expect("live summary is UTF-8");

    assert_eq!(checkpoint["schema"], "rhei.summary.checkpoint.v1", "{checkpoint:#}");
    assert_eq!(checkpoint["invocations"].as_array().map(Vec::len), Some(1), "{checkpoint:#}");
    assert_eq!(checkpoint["spawns"], serde_json::json!([]), "{checkpoint:#}");
    let states: Vec<(String, String)> = checkpoint["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| (task["id"].as_str().unwrap_or("").to_string(), task["state"].to_string()))
        .collect();
    assert_eq!(states.len(), 3, "every task in scope: {checkpoint:#}");
    assert!(live.contains("1 task completed, 2 in progress."), "live tally; got:\n{live}");

    let file = ran.lay.out.join("checkpoint.json").display().to_string();
    let replay = ran.summary(&["--details", "--checkpoint", &file]);
    assert_success(&replay);
    assert_eq!(replay.stdout, live, "the replay must be the live block, byte for byte");
    assert!(!replay.stdout.contains(&ran.lay.out.display().to_string()), "no checkpoint path");

    let now = ran.summary(&["--details"]);
    assert_ne!(now.stdout, live, "fixture: the plain summary did move after t3");
}

#[test]
fn a_checkpoint_carries_unrecorded_spawns_by_their_spawn_file_and_refuses_a_lost_one() {
    // §FS-rhei-summary.6.1 §FS-rhei-summary.5: an unrecorded spawn is a
    // member named by its spawn record's file name; once that record is gone,
    // the replay names it and refuses.
    let workspace = spawn_workspace("provenance-spawns", NOFAMILY_PLAN, NOFAMILY_MACHINE);
    workspace.run();
    workspace.assert_shape(4, 2);
    let summary = |args: &[&str]| {
        let mut cmd = rhei_command(workspace.root.join(".home"));
        cmd.env("GIT_CEILING_DIRECTORIES", &workspace.root);
        let out = cmd.arg("summary").arg(&workspace.plan).args(args).output().expect("summary");
        CliRun::from(&out)
    };

    let print = summary(&["--print-checkpoint"]);
    assert_success(&print);
    let doc: serde_json::Value = serde_json::from_str(&print.stdout)
        .unwrap_or_else(|err| panic!("checkpoint JSON ({err}):\n{}", print.stdout));
    let spawns = doc["spawns"].as_array().expect("spawns");
    assert_eq!(spawns.len(), 2, "the two `cdx` spawns: {doc:#}");
    for spawn in spawns {
        let name = spawn["spawn"].as_str().expect("spawn file name");
        assert!(workspace.root.join("runtime/spawns").join(name).is_file(), "{name}");
        assert_eq!(spawn["state"], "implement", "{doc:#}");
    }
    let file = workspace.root.join("checkpoint.json");
    fs::write(&file, &print.stdout).expect("save the checkpoint");
    let file_arg = file.display().to_string();

    let live = summary(&["--details"]);
    let replay = summary(&["--details", "--checkpoint", &file_arg]);
    assert_success(&replay);
    assert_eq!(replay.stdout, live.stdout, "nothing ran since: the replay is the live block");
    assert!(replay.stdout.contains("2 of them with no accounting record"), "{}", replay.stdout);

    let lost = spawns[0]["spawn"].as_str().expect("spawn name").to_string();
    fs::remove_file(workspace.root.join("runtime/spawns").join(&lost)).expect("lose a spawn");
    let refused = summary(&["--checkpoint", &file_arg]);
    assert!(!refused.status.success(), "a missing member is refused; got:\n{}", refused.stdout);
    assert!(refused.stderr.contains(&lost), "names the lost member; got:\n{}", refused.stderr);
}

#[test]
fn an_empty_summary_names_only_the_template() {
    // §FS-rhei-summary.5: zero invocations still say which template.
    let lay = Lay::new("provenance-empty", Some(TEMPLATE_REMOTE));
    assert_success(&lay.instantiate(&lay.template, &[]));
    let result = run(rhei_in(&lay.dir, &lay.dir), &["summary", "ws"]);
    assert_success(&result);
    let expected = format!(
        "Provenance: template `tiny` 1.0.0 @ example-org/templates@{} (clean) · \
         no invocations recorded",
        &lay.template_commit[..7]
    );
    assert_eq!(result.stdout.lines().last(), Some(expected.as_str()), "got:\n{}", result.stdout);
}

#[test]
fn checkpoint_flags_refuse_each_other_and_the_run_pairing() {
    // §FS-rhei-summary.1: usage errors that name both flags, not an unknown flag.
    let lay = Lay::new("provenance-refusals", None);
    assert_success(&lay.instantiate(&lay.template, &[]));
    fs::write(lay.dir.join("c.json"), "{}").expect("a checkpoint file");
    for args in [
        &["summary", "ws", "--checkpoint", "c.json", "--run", "abc123", "--prices", "p.json"][..],
        &["summary", "ws", "--print-checkpoint", "--checkpoint", "c.json"][..],
    ] {
        let result = run(rhei_in(&lay.dir, &lay.dir), args);
        assert!(!result.status.success(), "{args:?} must be refused");
        assert!(result.stderr.contains("cannot be used with"), "{args:?}: {}", result.stderr);
    }
}

#[test]
fn a_file_remote_prints_the_bare_commit_and_no_path() {
    // §FS-rhei-summary.4: a remote that is a path reduces to nothing.
    let dir = unique_temp_dir("provenance-file-remote-root");
    let remote = format!("file://{}", dir.join("remote.git").display());
    let ran = ran("provenance-file-remote", &remote);
    let result = ran.summary(&["--details"]);
    assert_success(&result);
    let line = result.stdout.lines().find(|line| line.starts_with("Provenance: ")).unwrap_or("");
    assert!(
        line.contains(&format!("checkout HEAD at start {} ·", &ran.head[..7])),
        "got:\n{}",
        result.stdout
    );
    for path in [ran.lay.dir.display().to_string(), dir.display().to_string()] {
        assert!(!result.stdout.contains(&path), "path {path} leaked:\n{}", result.stdout);
    }
}
