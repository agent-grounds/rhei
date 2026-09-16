// Subtree supervision: the hold/release barrier a task in an `execute_on:` state
// puts over its own subtree, the checkpoints its descendants deliver to it, and
// the `supervision` frontmatter block that survives a stopped run.
//
// Its own part because supervision is one rule read from four places — the
// shared transition path writes it, the ready set and `rhei next` read it,
// prompt composition renders it — and none of those files owns it.

// §AR-source-file-size.3 §FS-rhei-supervision

/// The `metadata.tasks.<id>.supervision` key. §FS-rhei-supervision.3.3
const SUPERVISION_KEY: &str = "supervision";

/// Where a supervisor stands between visits.
///
/// `Held` is the default for a task in a supervising state with no block of its
/// own: a task authored straight into a supervising state is owed its first
/// visit, and nothing beneath it may run until it has had one.
// §FS-rhei-supervision.3.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SupervisionPhase {
    /// The supervisor is owed a visit; nothing beneath it is dispatched.
    Held,
    /// The supervisor took its self-loop; the subtree follows the ordinary rules.
    Released,
}

impl SupervisionPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Released => "released",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "held" => Some(Self::Held),
            "released" => Some(Self::Released),
            _ => None,
        }
    }
}

/// One delivered checkpoint: which descendant moved, between which states, and
/// which visit of the target state that was. §FS-rhei-supervision.3.3
#[derive(Debug, Clone, PartialEq, Eq)]
struct SupervisionCheckpoint {
    /// The rhei-local id of the transitioning descendant.
    task: String,
    from: String,
    to: String,
    visit: u64,
}

/// Every non-terminal descendant of `task`, at any depth.
///
/// A subtree lives inside one rhei, so the transitioning task's own machine
/// judges every node under it.
// §FS-rhei-supervision.4.1 §DA-per-rhei-state-machines: what `openDescendants` counts.
fn open_descendant_count(task: &rhei_core::ast::Task, machine: &rhei_validator::StateMachine) -> u64 {
    let mut count = 0;
    for child in &task.children {
        let state = normalized_state_name(child.state.as_str(), machine);
        if !machine.states.get(&state).map(|def| def.terminal).unwrap_or(false) {
            count += 1;
        }
        count += open_descendant_count(child, machine);
    }
    count
}

/// The scope and event a state executes on, or `None` when it does not
/// supervise.
// §FS-rhei-supervision.1.1
fn execute_on_of(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
) -> Option<rhei_validator::ExecuteOn> {
    machine.states.get(state_name).and_then(|def| def.execute_on())
}

/// Whether the state a task is *currently in* supervises.
// §FS-rhei-supervision.1.1: `execute_on` is a property of the state, not the task.
fn task_is_supervising(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> bool {
    execute_on_of(machine, &normalized_state_name(task.state.as_str(), machine)).is_some()
}

// ---------------------------------------------------------------------------
// Reading the metadata block
// ---------------------------------------------------------------------------

fn supervision_map<'a>(metadata: Option<&'a Metadata>, task_id: &TaskId) -> Option<&'a YamlMapping> {
    task_metadata_map(metadata, task_id)?.get(yaml_key(SUPERVISION_KEY))?.as_mapping()
}

/// The phase written in the plan, or `None` when the task carries no block.
///
/// The block is the hold, not the state: a supervisor that left its supervising
/// state for a human gate keeps the block, and its subtree stays held until a
/// human moves it. Only a reader that can tell "held" from "no block at all"
/// can implement that.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.3
fn recorded_supervision_phase(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
) -> Option<SupervisionPhase> {
    supervision_map(metadata, task_id)
        .and_then(|map| map.get(yaml_key("phase")))
        .and_then(YamlValue::as_str)
        .and_then(SupervisionPhase::parse)
}

/// The recorded phase of a task in a supervising state.
///
/// A missing block reads as `Held`, which is what makes an authored initial
/// supervising state hold its subtree without anything having written to the
/// plan first.
// §FS-rhei-supervision.3.3
fn supervision_phase(metadata: Option<&Metadata>, task_id: &TaskId) -> SupervisionPhase {
    recorded_supervision_phase(metadata, task_id).unwrap_or(SupervisionPhase::Held)
}

/// The checkpoints delivered since the supervisor's last visit, in delivery
/// order. §FS-rhei-supervision.3.3
fn supervision_checkpoints(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
) -> Vec<SupervisionCheckpoint> {
    let Some(entries) = supervision_map(metadata, task_id)
        .and_then(|map| map.get(yaml_key("checkpoints")))
        .and_then(YamlValue::as_sequence)
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let map = entry.as_mapping()?;
            let text = |key: &str| {
                map.get(yaml_key(key)).and_then(|value| match value {
                    YamlValue::String(value) => Some(value.clone()),
                    YamlValue::Number(value) => Some(value.to_string()),
                    _ => None,
                })
            };
            Some(SupervisionCheckpoint {
                task: text("task")?,
                from: text("from").unwrap_or_default(),
                to: text("to").unwrap_or_default(),
                visit: map
                    .get(yaml_key("visit"))
                    .and_then(yaml_value_to_u64)
                    .unwrap_or(1),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Writing the metadata block
// ---------------------------------------------------------------------------

fn checkpoint_yaml(checkpoint: &SupervisionCheckpoint) -> YamlValue {
    let mut map = YamlMapping::new();
    map.insert(yaml_key("task"), yaml_key(&checkpoint.task));
    map.insert(yaml_key("from"), yaml_key(&checkpoint.from));
    map.insert(yaml_key("to"), yaml_key(&checkpoint.to));
    map.insert(yaml_key("visit"), yaml_u64(checkpoint.visit));
    YamlValue::Mapping(map)
}

/// Write `phase: held`, appending `checkpoint` to the pending list when one was
/// delivered. Entry into a supervising state passes `None` and starts a fresh
/// list. §FS-rhei-supervision.3.3
fn record_supervision_hold(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    checkpoint: Option<&SupervisionCheckpoint>,
) -> Metadata {
    let carried: Vec<YamlValue> = match checkpoint {
        Some(_) => supervision_map(existing, task_id)
            .and_then(|map| map.get(yaml_key("checkpoints")))
            .and_then(YamlValue::as_sequence)
            .cloned()
            .unwrap_or_default(),
        None => Vec::new(),
    };

    let mut root = existing.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let supervision = ensure_mapping(task_entry, yaml_key(SUPERVISION_KEY));
    supervision.insert(yaml_key("phase"), yaml_key(SupervisionPhase::Held.as_str()));
    let mut list = carried;
    if let Some(checkpoint) = checkpoint {
        list.push(checkpoint_yaml(checkpoint));
    }
    if list.is_empty() {
        supervision.remove(yaml_key("checkpoints"));
    } else {
        supervision.insert(yaml_key("checkpoints"), YamlValue::Sequence(list));
    }
    root
}

/// Write `phase: released` and drop the checkpoints the visit just consumed.
// §FS-rhei-supervision.3.1: the self-loop is the release edge.
fn record_supervision_release(existing: Option<&Metadata>, task_id: &TaskId) -> Metadata {
    let mut root = existing.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let supervision = ensure_mapping(task_entry, yaml_key(SUPERVISION_KEY));
    supervision.insert(yaml_key("phase"), yaml_key(SupervisionPhase::Released.as_str()));
    supervision.remove(yaml_key("checkpoints"));
    root
}

/// Drop one task's supervision block: it left the supervising state by an edge
/// that is not the self-loop. §FS-rhei-supervision.3.1
fn clear_supervision_for_task(existing: Option<&Metadata>, task_id: &TaskId) -> Option<Metadata> {
    let mut root = existing.cloned()?;
    let YamlValue::Mapping(metadata_section) = root.get_mut(yaml_key("metadata"))? else {
        return Some(root);
    };
    let YamlValue::Mapping(tasks) = metadata_section.get_mut(yaml_key("tasks"))? else {
        return Some(root);
    };
    let YamlValue::Mapping(task_entry) = tasks.get_mut(task_id_yaml_key(task_id))? else {
        return Some(root);
    };
    task_entry.remove(yaml_key(SUPERVISION_KEY));
    Some(root)
}

/// The runtime task metadata `rhei reset` clears: the visit counters, and the
/// supervision blocks that are meaningless without them.
///
/// A task whose whole entry was runtime state loses the entry too. `tasks: {1:
/// {}}` in a reset plan is a record of nothing, and the next reader has to
/// decide whether it means anything.
// §FS-rhei-supervision.3.3 §FS-rhei-reset
fn clear_runtime_task_metadata(existing: Option<&Metadata>) -> Option<Metadata> {
    let without_visits = clear_runtime_state_visits(existing)?;
    let without_limits = clear_runtime_provider_limits(Some(&without_visits))?;
    let cleared = clear_runtime_supervision(Some(&without_limits))?;
    Some(drop_empty_task_metadata(cleared))
}

/// Drop `metadata.tasks` entries left empty by a clear, and the containers left
/// empty by that. §FS-rhei-reset
fn drop_empty_task_metadata(mut root: Metadata) -> Metadata {
    let Some(YamlValue::Mapping(metadata_section)) = root.get_mut(yaml_key("metadata")) else {
        return root;
    };
    if let Some(YamlValue::Mapping(tasks)) = metadata_section.get_mut(yaml_key("tasks")) {
        tasks.retain(|_, value| !matches!(value, YamlValue::Mapping(map) if map.is_empty()));
        if tasks.is_empty() {
            metadata_section.remove(yaml_key("tasks"));
        }
    }
    if metadata_section.is_empty() {
        root.remove(yaml_key("metadata"));
    }
    root
}

/// Drop every task's supervision block, beside the `stateVisits` reset.
// §FS-rhei-supervision.3.3 §FS-rhei-reset
fn clear_runtime_supervision(existing: Option<&Metadata>) -> Option<Metadata> {
    let mut root = existing.cloned()?;
    let Some(YamlValue::Mapping(metadata_section)) = root.get_mut(yaml_key("metadata")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(tasks)) = metadata_section.get_mut(yaml_key("tasks")) else {
        return Some(root);
    };
    for value in tasks.values_mut() {
        if let YamlValue::Mapping(task_map) = value {
            task_map.remove(yaml_key(SUPERVISION_KEY));
        }
    }
    Some(root)
}
