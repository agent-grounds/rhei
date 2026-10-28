// A summary frozen as the set it covers, and the line that says where that set
// came from. `--print-checkpoint` prints the set, `--checkpoint` prints the
// summary of exactly it again, and every summary ends with the provenance line
// naming its checkpoint id.

// §FS-rhei-summary.6 §FS-rhei-summary.2.4

/// The schema of the checkpoint document. §FS-rhei-summary.6
const SUMMARY_CHECKPOINT_SCHEMA: &str = "rhei.summary.checkpoint.v1";

/// What `--print-checkpoint` prints and `--checkpoint` reads.
/// §FS-rhei-summary.6.1
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SummaryCheckpoint {
    schema: String,
    invocations: Vec<String>,
    spawns: Vec<CheckpointSpawn>,
    tasks: Vec<CheckpointTask>,
}

/// One unrecorded agent spawn in the set, by its spawn record's file name.
/// §FS-rhei-summary.6.1
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointSpawn {
    task: String,
    state: String,
    moves: u64,
    attempt: u64,
    spawn: String,
}

/// One task in scope and the state it was in when the document was taken.
/// §FS-rhei-summary.6.1
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointTask {
    id: String,
    state: String,
}

impl SummaryCheckpoint {
    /// The set an ordinary summary of `inspection` covers right now, with the
    /// scope's tasks in their current states. §FS-rhei-summary.6.1
    fn of(
        inspection: &CostInspection,
        rhei: &rhei_core::ast::Rhei,
        scope: &RheiScope,
    ) -> SummaryCheckpoint {
        let tasks = narrow_to_rhei_scope(flatten_tasks(rhei), scope)
            .into_iter()
            .map(|task| CheckpointTask { id: task.id.to_string(), state: task.state.clone() })
            .collect();
        SummaryCheckpoint {
            schema: SUMMARY_CHECKPOINT_SCHEMA.to_string(),
            invocations: inspection
                .invocations
                .iter()
                .map(|held| held.record.invocation_id.clone())
                .collect(),
            spawns: inspection
                .unrecorded
                .iter()
                .map(|spawn| CheckpointSpawn {
                    task: spawn.task.clone(),
                    state: spawn.state.clone(),
                    moves: spawn.moves,
                    attempt: spawn.attempt,
                    spawn: spawn.spawn.clone(),
                })
                .collect(),
            tasks,
        }
        .sorted()
    }

    /// The arrays in the order §FS-rhei-summary.6.2 fixes.
    fn sorted(mut self) -> SummaryCheckpoint {
        self.invocations.sort();
        self.invocations.dedup();
        self.spawns.sort_by(|a, b| a.spawn.cmp(&b.spawn));
        self.tasks.sort_by(|a, b| a.id.cmp(&b.id));
        self
    }

    /// The canonical bytes: sorted keys, no insignificant whitespace, one
    /// trailing newline. §FS-rhei-summary.6.2
    fn canonical(&self) -> String {
        let value = serde_json::to_value(self.clone().sorted()).unwrap_or_default();
        format!("{}\n", canonical_json_text(&value))
    }

    /// The first 12 hex digits of the sha256 of the canonical bytes.
    /// §FS-rhei-summary.6.2
    fn id(&self) -> String {
        sha256_hex(self.canonical().as_bytes())[..12].to_string()
    }
}

/// Read the document `--checkpoint` names, `-` being stdin. Every failure names
/// the file as given. §FS-rhei-summary.5
fn read_summary_checkpoint(given: &str) -> MietteResult<SummaryCheckpoint> {
    let text = if given == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .map_err(|err| miette!(help = "pipe the document `--print-checkpoint` printed", "checkpoint {given}: cannot be read: {err}"))?;
        text
    } else {
        fs::read_to_string(given).map_err(|err| {
            miette!(help = "name the file `--print-checkpoint` was redirected to", "checkpoint {given}: cannot be read: {err}")
        })?
    };
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| {
            miette!(help = "pass a document `rhei summary --print-checkpoint` printed", "checkpoint {given}: is not JSON: {err}")
        })?;
    if value.get("schema").and_then(serde_json::Value::as_str) != Some(SUMMARY_CHECKPOINT_SCHEMA) {
        return Err(miette!(
            help = "pass a document `rhei summary --print-checkpoint` printed",
            "checkpoint {given}: its schema is not {SUMMARY_CHECKPOINT_SCHEMA}"
        ));
    }
    serde_json::from_value(value)
        .map_err(|err| {
            miette!(help = "take the checkpoint again with `rhei summary --print-checkpoint`", "checkpoint {given}: is not a {SUMMARY_CHECKPOINT_SCHEMA} document: {err}")
        })
}

/// Narrow a live reading to exactly the checkpoint's members: its records by
/// `invocation_id`, its unrecorded spawns read again from their spawn records,
/// and the aggregate recomputed over them alone. A member that is gone is an
/// error naming it. §FS-rhei-summary.1 §FS-rhei-summary.5
fn replay_summary_checkpoint(
    inspection: CostInspection,
    checkpoint: &SummaryCheckpoint,
    given: &str,
    spawn_roots: &[PathBuf],
) -> MietteResult<CostInspection> {
    let wanted: BTreeSet<&str> = checkpoint.invocations.iter().map(String::as_str).collect();
    let present: BTreeSet<&str> =
        inspection.invocations.iter().map(|held| held.record.invocation_id.as_str()).collect();
    if let Some(missing) = wanted.difference(&present).next() {
        return Err(missing_checkpoint_member(given, &format!("invocation {missing}")));
    }
    let mut on_disk: BTreeMap<String, SpawnRecordFacts> = BTreeMap::new();
    for root in spawn_roots {
        for spawn in read_agent_spawns(root).0 {
            on_disk.entry(spawn.spawn.clone()).or_insert(spawn);
        }
    }
    let mut unrecorded = Vec::new();
    for member in &checkpoint.spawns {
        // A later attempt of the visit rewrites the record under the same name,
        // so the name alone does not prove it is the member. §FS-rhei-summary.5
        let Some(facts) = on_disk.get(&member.spawn).filter(|facts| {
            (facts.task.as_str(), facts.state.as_str(), facts.moves, facts.attempt)
                == (member.task.as_str(), member.state.as_str(), member.moves, member.attempt)
        }) else {
            return Err(missing_checkpoint_member(given, &format!("spawn {}", member.spawn)));
        };
        unrecorded.push(UnrecordedSpawn::from_facts(facts.clone()));
    }
    sort_unrecorded(&mut unrecorded);
    let invocations = inspection
        .invocations
        .into_iter()
        .filter(|held| wanted.contains(held.record.invocation_id.as_str()))
        .collect();
    let replayed = CostInspection { summary: None, invocations, unrecorded, ..inspection };
    let summary = summarize_records(replayed.scoped()).map(|summary| {
        let summary = demote_if(summary, replayed.unreadable_root);
        count_unrecorded(summary, replayed.unrecorded.len())
    });
    Ok(CostInspection { summary, ..replayed })
}

fn missing_checkpoint_member(given: &str, member: &str) -> miette::Report {
    miette!(
        help = "a reset or a later attempt removed it; take a new checkpoint",
        "checkpoint {given}: missing member {member}: nothing in the selected roots carries it"
    )
}

/// `Provenance: rhei <build> · checkout HEAD at start <head> · template <tmpl> ·
/// checkpoint <id>: <runs><n> invocation[s]`, or the template alone when the
/// reading has no member. §FS-rhei-summary.2.4 §FS-rhei-summary.5
fn summary_provenance_line(inspection: &CostInspection, checkpoint_id: &str, template: &str) -> String {
    let members = inspection.invocations.len() + inspection.unrecorded.len();
    if members == 0 {
        return format!("Provenance: template {template} · no invocations recorded");
    }
    let mut builds: Vec<String> = Vec::new();
    for step in summary_step_order(inspection) {
        let build = match step {
            SummaryStep::Record(held) => rhei_build_label(held.record.rhei_build.as_ref()),
            SummaryStep::Unrecorded(_) => "unknown".to_string(),
        };
        if !builds.contains(&build) {
            builds.push(build);
        }
    }
    // The reading is in `started_at` order, so its first record started first.
    let head = inspection
        .invocations
        .first()
        .and_then(|held| held.record.checkout_head_at_start.clone().flatten())
        .map(|head| match head.repo {
            Some(repo) => format!("{repo}@{}", short_commit(&head.commit)),
            None => short_commit(&head.commit).to_string(),
        })
        .unwrap_or_else(|| "unknown".to_string());
    format!(
        "Provenance: rhei {} · checkout HEAD at start {head} · template {template} · \
         checkpoint {checkpoint_id}: {}{members} invocation{}",
        builds.join(", "),
        provenance_runs(inspection),
        plural_s(members),
    )
}

/// `<version> @ <sha7>`, ` (dirty)` after a dirty build, `<version> @ unknown`
/// with no commit, and `unknown` for a record that carries no build.
/// §FS-rhei-summary.2.4
fn rhei_build_label(build: Option<&rhei_tui::RheiBuild>) -> String {
    let Some(build) = build else { return "unknown".to_string() };
    match &build.commit {
        Some(commit) if build.dirty == Some(true) => {
            format!("{} @ {} (dirty)", build.version, short_commit(commit))
        }
        Some(commit) => format!("{} @ {}", build.version, short_commit(commit)),
        None => format!("{} @ unknown", build.version),
    }
}

/// The distinct `run_id`s of the member records, cut to six characters, in the
/// order the runs started: `run a, `, `runs a, b, `, or nothing.
/// §FS-rhei-summary.2.4
fn provenance_runs(inspection: &CostInspection) -> String {
    let mut first_seen: BTreeMap<&str, &str> = BTreeMap::new();
    for held in &inspection.invocations {
        let Some(run_id) = held.record.run_id.as_deref() else { continue };
        let started = held.record.started_at.as_str();
        let entry = first_seen.entry(run_id).or_insert(started);
        if started < *entry {
            *entry = started;
        }
    }
    let mut runs: Vec<(&str, &str)> = first_seen.into_iter().map(|(id, at)| (at, id)).collect();
    runs.sort();
    let ids: Vec<&str> = runs.iter().map(|(_, id)| id.get(..6).unwrap_or(id)).collect();
    match ids.len() {
        0 => String::new(),
        1 => format!("run {}, ", ids[0]),
        _ => format!("runs {}, ", ids.join(", ")),
    }
}

fn short_commit(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

/// The `<tmpl>` field, read from the last `laid` fence of the machine the lead
/// names, or `unknown` when it has none. §FS-rhei-summary.2.4 §FS-rhei-library.7.1
fn summary_template_provenance(machine_path: Option<&Path>) -> String {
    machine_path
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| {
            text.lines().rev().find_map(|line| laid_fence_provenance(line.trim_end()))
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// One `# --- laid <name> <version> src:sha256:<hex> [git:<g>] <state>
/// inputs:sha256:<hex> ---` line, spelled as the provenance line names it.
/// §FS-rhei-summary.2.4
fn laid_fence_provenance(line: &str) -> Option<String> {
    let body = line.strip_prefix("# --- laid ")?.strip_suffix(" ---")?;
    let fields: Vec<&str> = body.split_whitespace().collect();
    let (name, version) = (fields.first()?, fields.get(1)?);
    let src = fields.get(2)?.strip_prefix("src:sha256:")?;
    let (git, state) = match fields.get(3)?.strip_prefix("git:") {
        Some(git) => (Some(git), *fields.get(4)?),
        None => (None, fields[3]),
    };
    if state == "builtin" {
        return Some(format!("`{name}` {version} (built into rhei)"));
    }
    Some(match git {
        Some(git) => {
            let at = match git.rsplit_once('@') {
                Some((repo, commit)) => format!("{repo}@{}", short_commit(commit)),
                None => short_commit(git).to_string(),
            };
            format!("`{name}` {version} @ {at} ({state})")
        }
        None => format!("`{name}` {version} src:sha256:{} ({state})", src.get(..12).unwrap_or(src)),
    })
}
