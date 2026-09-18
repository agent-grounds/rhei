//! Typed composition provenance and its canonical v1 lock encoding.
//! §FS-rhei-library.4.1 §AR-rhei-library.2–5
use super::references::{tasks, Names};
use super::*;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const LOCK_PATH: &str = ".agent-grounds/rhei/composition.lock.json";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct MountRecord {
    alias: Option<String>,
    chain: Vec<String>,
    encoded: String,
    source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct DeclarationRecord {
    file: String,
    kind: String,
    local: Option<String>,
    occurrence: Option<usize>,
    rendered: String,
    source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Origin {
    declaration: String,
    mount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    via: String,
}

#[derive(Debug, Clone, Default, Serialize)]
struct NodeTables {
    profiles: BTreeMap<String, Vec<Origin>>,
    routing: BTreeMap<String, Vec<Origin>>,
    states: BTreeMap<String, Vec<Origin>>,
    tasks: BTreeMap<String, Vec<Origin>>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProvenanceStore {
    sources: BTreeMap<String, SourceIdentity>,
    mounts: BTreeMap<String, MountRecord>,
    declarations: BTreeMap<String, DeclarationRecord>,
    nodes: NodeTables,
    routing_values: BTreeMap<String, Value>,
}

#[derive(Serialize)]
struct CompositionLock<'a> {
    declarations: &'a BTreeMap<String, DeclarationRecord>,
    mounts: &'a BTreeMap<String, MountRecord>,
    nodes: &'a NodeTables,
    schema_version: u8,
    sources: &'a BTreeMap<String, SourceIdentity>,
}

impl ProvenanceStore {
    pub(crate) fn attach(
        &mut self,
        source: &SourceIdentity,
        chain: &[String],
        fragment: Option<&Fragment>,
    ) -> CompileResult<()> {
        let source_id = source.id()?;
        insert_equal(&mut self.sources, source_id.clone(), source.clone(), "source")?;
        let encoded = Qualifier::new(chain.to_vec()).prefix();
        let mount = if chain.is_empty() { "root".into() } else { encoded.clone() };
        insert_equal(
            &mut self.mounts,
            mount.clone(),
            MountRecord {
                alias: chain.last().cloned(),
                chain: chain.to_vec(),
                encoded,
                source: source_id.clone(),
            },
            "mount",
        )?;
        if let Some(fragment) = fragment {
            self.capture_fragment(fragment, &mount, &source_id)?;
        }
        Ok(())
    }

    fn capture_fragment(
        &mut self,
        fragment: &Fragment,
        mount: &str,
        source: &str,
    ) -> CompileResult<()> {
        for (name, state) in &fragment.machine.states {
            let declaration = self.declare(
                "state",
                "states.yaml",
                Some(name),
                serde_json::to_value(state).map_err(|e| e.to_string())?,
                None,
                source,
            )?;
            add_origin(&mut self.nodes.states, name, direct(mount, &declaration));
        }
        for file in &fragment.tasks {
            tasks(&file.tasks, &mut |task| {
                let rendered = json!({
                    "assignee": task.assignee,
                    "consumes": task.consumes.iter().map(|value| format!("{}:{}", value.task, value.name)).collect::<Vec<_>>(),
                    "content": task.content,
                    "excludes": task.excludes.iter().map(|value| value.authored()).collect::<Vec<_>>(),
                    "id": task.id.to_string(),
                    "kind": task.kind,
                    "model": task.model,
                    "prior": task.prior.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "prior_kinds": task.prior_kinds,
                    "provides": task.provides,
                    "state": task.state,
                    "target": task.target,
                    "title": task.title,
                });
                if let Ok(declaration) = self.declare(
                    "task",
                    &slash(&file.path),
                    Some(&task.id.to_string()),
                    rendered.clone(),
                    None,
                    source,
                ) {
                    add_origin(
                        &mut self.nodes.tasks,
                        &task.id.to_string(),
                        direct(mount, &declaration),
                    );
                }
            });
        }
        for (name, profile) in fragment.machine.profiles.iter().flat_map(|p| p.iter()) {
            let declaration = self.declare(
                "profile",
                "states.yaml",
                Some(name),
                serde_json::to_value(profile).map_err(|e| e.to_string())?,
                None,
                source,
            )?;
            add_origin(&mut self.nodes.profiles, name, direct(mount, &declaration));
        }
        if let Some(policy) = &fragment.machine.node_policy {
            for (key, value) in
                [("root", &policy.root), ("rhei", &policy.default), ("default", &policy.default)]
            {
                let declaration =
                    self.declare("routing", "states.yaml", Some(key), json!(value), None, source)?;
                add_origin(&mut self.nodes.routing, key, direct(mount, &declaration));
            }
            for (kind, profile) in &policy.by_type {
                let key = format!("by_type/{kind}");
                let declaration = self.declare(
                    "routing",
                    "states.yaml",
                    Some(&key),
                    json!([kind, profile]),
                    None,
                    source,
                )?;
                add_origin(&mut self.nodes.routing, &key, direct(mount, &declaration));
            }
            let mut duplicate = BTreeMap::<String, usize>::new();
            for rule in &policy.overrides {
                let rendered = serde_json::to_value(rule).map_err(|e| e.to_string())?;
                let rendered_digest = digest_value(&rendered)?;
                let occurrence = duplicate.entry(rendered_digest.clone()).or_default();
                *occurrence += 1;
                let declaration = self.declare(
                    "routing",
                    "states.yaml",
                    None,
                    rendered.clone(),
                    Some(*occurrence),
                    source,
                )?;
                let key = format!("overrides/sha256:{rendered_digest}~{occurrence}");
                self.routing_values.insert(key.clone(), rendered);
                add_origin(&mut self.nodes.routing, &key, direct(mount, &declaration));
            }
        }
        Ok(())
    }

    fn declare(
        &mut self,
        kind: &str,
        file: &str,
        local: Option<&str>,
        rendered: Value,
        occurrence: Option<usize>,
        source: &str,
    ) -> CompileResult<String> {
        let rendered_digest = format!("sha256:{}", digest_value(&rendered)?);
        // Source identity disambiguates byte-identical declarations from
        // independent blocks so each normalized record has one honest source.
        // §FS-rhei-library.4.1
        let tuple = json!([source, kind, file, local, rendered_digest, occurrence]);
        let id = format!("decl:{kind}:sha256:{}", digest_value(&tuple)?);
        let record = DeclarationRecord {
            file: file.into(),
            kind: kind.into(),
            local: local.map(str::to_string),
            occurrence,
            rendered: rendered_digest,
            source: source.into(),
        };
        insert_equal(&mut self.declarations, id.clone(), record, "declaration")?;
        Ok(id)
    }

    pub(crate) fn rewrite(&mut self, names: &Names) -> CompileResult<()> {
        rename_nodes(&mut self.nodes.states, &names.states);
        rename_nodes(&mut self.nodes.tasks, &names.tasks);
        rename_nodes(&mut self.nodes.profiles, &names.profiles);
        let routing = std::mem::take(&mut self.nodes.routing);
        let mut values = std::mem::take(&mut self.routing_values);
        for (mut key, origins) in routing {
            if let Some(kind) = key.strip_prefix("by_type/") {
                if let Some(qualified) = names.kinds.get(kind) {
                    key = format!("by_type/{qualified}");
                }
            } else if key.starts_with("overrides/") {
                let mut rendered = values
                    .remove(&key)
                    .ok_or_else(|| format!("missing rendered routing provenance '{key}'"))?;
                rewrite_routing_value(&mut rendered, names);
                let occurrence = key.rsplit_once('~').map(|(_, value)| value).unwrap_or("1");
                key = format!("overrides/sha256:{}~{occurrence}", digest_value(&rendered)?);
                insert_equal(
                    &mut self.routing_values,
                    key.clone(),
                    rendered,
                    "routing provenance",
                )?;
            }
            add_origins(&mut self.nodes.routing, key, origins);
        }
        Ok(())
    }

    pub(crate) fn merge(&mut self, other: Self) -> CompileResult<()> {
        merge_equal(&mut self.sources, other.sources, "source")?;
        merge_equal(&mut self.mounts, other.mounts, "mount")?;
        merge_equal(&mut self.declarations, other.declarations, "declaration")?;
        merge_equal(&mut self.routing_values, other.routing_values, "routing provenance")?;
        merge_nodes(&mut self.nodes.states, other.nodes.states);
        merge_nodes(&mut self.nodes.tasks, other.nodes.tasks);
        merge_nodes(&mut self.nodes.profiles, other.nodes.profiles);
        merge_nodes(&mut self.nodes.routing, other.nodes.routing);
        Ok(())
    }

    pub(crate) fn finalize(&mut self, fragment: &Fragment, flow: &str) {
        let contributors = self
            .nodes
            .states
            .values()
            .chain(self.nodes.tasks.values())
            .chain(self.nodes.profiles.values())
            .chain(self.nodes.routing.values())
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.nodes.states.retain(|name, _| fragment.machine.states.contains_key(name));
        let task_ids = fragment
            .tasks
            .iter()
            .flat_map(|file| {
                let mut ids = Vec::new();
                tasks(&file.tasks, &mut |task| ids.push(task.id.to_string()));
                ids
            })
            .collect::<BTreeSet<_>>();
        self.nodes.tasks.retain(|name, _| task_ids.contains(name));
        let profiles = fragment
            .machine
            .profiles
            .iter()
            .flat_map(|profiles| profiles.keys().cloned())
            .collect::<BTreeSet<_>>();
        self.nodes.profiles.retain(|name, _| profiles.contains(name));
        let synthesized = contributors
            .iter()
            .map(|origin| Origin {
                declaration: origin.declaration.clone(),
                mount: origin.mount.clone(),
                reason: Some("derived-composition-routing".into()),
                via: "synthesized".into(),
            })
            .collect::<Vec<_>>();
        self.nodes.profiles.insert(flow.into(), canonical(synthesized.clone()));
        for key in ["root", "rhei", "default"] {
            self.nodes.routing.insert(key.into(), canonical(synthesized.clone()));
        }
        for name in fragment.machine.states.keys() {
            ensure_origins(&mut self.nodes.states, name, &contributors);
        }
        for name in task_ids {
            ensure_origins(&mut self.nodes.tasks, &name, &contributors);
        }
        for name in profiles {
            ensure_origins(&mut self.nodes.profiles, &name, &contributors);
        }
    }

    pub(crate) fn lock_bytes(&self) -> CompileResult<Vec<u8>> {
        let lock = CompositionLock {
            declarations: &self.declarations,
            mounts: &self.mounts,
            nodes: &self.nodes,
            schema_version: 1,
            sources: &self.sources,
        };
        let value = serde_json::to_value(lock).map_err(|e| e.to_string())?;
        let mut bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub(crate) fn header_lines(&self) -> Vec<String> {
        self.mounts
            .iter()
            .map(|(mount, record)| {
                format!(
                    "{} source={} alias={} lock={LOCK_PATH}",
                    mount,
                    record.source,
                    if record.chain.is_empty() { "root".into() } else { record.chain.join(".") }
                )
            })
            .collect()
    }
}

fn rewrite_routing_value(value: &mut Value, names: &Names) {
    let Some(rule) = value.as_object_mut() else { return };
    let kind = rule
        .get("match")
        .and_then(Value::as_object)
        .and_then(|match_| match_.get("type"))
        .and_then(Value::as_str)
        .and_then(|kind| names.kinds.get(kind))
        .cloned();
    if let Some(kind) = kind {
        rule["match"]["type"] = Value::String(kind);
    }
    let profile = rule
        .get("profile")
        .and_then(Value::as_str)
        .and_then(|profile| names.profiles.get(profile))
        .cloned();
    if let Some(profile) = profile {
        rule["profile"] = Value::String(profile);
    }
}

fn direct(mount: &str, declaration: &str) -> Origin {
    Origin {
        declaration: declaration.into(),
        mount: mount.into(),
        reason: None,
        via: "declared".into(),
    }
}

fn ensure_origins(
    table: &mut BTreeMap<String, Vec<Origin>>,
    name: &str,
    contributors: &BTreeSet<Origin>,
) {
    if table.get(name).is_none_or(Vec::is_empty) {
        table.insert(
            name.into(),
            canonical(
                contributors
                    .iter()
                    .map(|origin| Origin {
                        declaration: origin.declaration.clone(),
                        mount: origin.mount.clone(),
                        reason: Some("compiler-generated-node".into()),
                        via: "synthesized".into(),
                    })
                    .collect(),
            ),
        );
    }
}

fn rename_nodes(table: &mut BTreeMap<String, Vec<Origin>>, names: &BTreeMap<String, String>) {
    let original = std::mem::take(table);
    for (name, origins) in original {
        let name = names.get(&name).cloned().unwrap_or(name);
        add_origins(table, name, origins);
    }
}

fn merge_nodes(into: &mut BTreeMap<String, Vec<Origin>>, from: BTreeMap<String, Vec<Origin>>) {
    for (name, origins) in from {
        add_origins(into, name, origins);
    }
}

fn add_origin(table: &mut BTreeMap<String, Vec<Origin>>, name: &str, origin: Origin) {
    add_origins(table, name.into(), vec![origin]);
}

fn add_origins(table: &mut BTreeMap<String, Vec<Origin>>, name: String, origins: Vec<Origin>) {
    let target = table.entry(name).or_default();
    target.extend(origins);
    *target = canonical(std::mem::take(target));
}

fn canonical(mut origins: Vec<Origin>) -> Vec<Origin> {
    origins.sort_by(|a, b| {
        (&a.mount, &a.declaration, &a.via, &a.reason).cmp(&(
            &b.mount,
            &b.declaration,
            &b.via,
            &b.reason,
        ))
    });
    origins.dedup();
    origins
}

fn insert_equal<T: PartialEq>(
    into: &mut BTreeMap<String, T>,
    key: String,
    value: T,
    kind: &str,
) -> CompileResult<()> {
    if let Some(existing) = into.get(&key) {
        if existing != &value {
            return Err(format!("{kind} identity collision '{key}'"));
        }
    } else {
        into.insert(key, value);
    }
    Ok(())
}

fn merge_equal<T: PartialEq>(
    into: &mut BTreeMap<String, T>,
    from: BTreeMap<String, T>,
    kind: &str,
) -> CompileResult<()> {
    for (key, value) in from {
        insert_equal(into, key, value, kind)?;
    }
    Ok(())
}

fn slash(path: &std::path::Path) -> String {
    path.components()
        .filter_map(|part| match part {
            std::path::Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn digest_value(value: &Value) -> CompileResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_fingerprint_has_a_fixed_canonical_vector() {
        let tuple =
            json!(["src:sha256:source", "state", "states.yaml", "review", "sha256:abc", null]);
        assert_eq!(
            digest_value(&tuple).unwrap(),
            "7340caae1016b23f4e569f34fca6829b697ba9f4942c022d0d9b0b4c6f431cef"
        );
    }

    #[test]
    fn canonical_origins_sort_and_deduplicate_without_losing_contributors() {
        let a = direct("m1_a__", "decl:state:a");
        let b = direct("m1_b__", "decl:state:b");
        assert_eq!(
            canonical(vec![b.clone(), a.clone(), a]),
            vec![direct("m1_a__", "decl:state:a"), b]
        );
    }
}
