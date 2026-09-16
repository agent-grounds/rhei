//! Typed expansion and lowering. §AR-rhei-library.2–3
use super::*;
use crate::ast::{Rhei, Structure, Task};
use crate::state_machine::{NodePolicy, Profile, StateMachine};
use indexmap::IndexMap;
use std::path::PathBuf;

pub type CompileResult<T> = Result<T, String>;

/// An already rendered and parsed local contribution. Text stays opaque;
/// semantic edits operate on the shared parser and validator structures.
#[derive(Debug, Clone)]
pub struct Fragment {
    pub machine: StateMachine,
    pub plan: Rhei,
    pub tasks: Vec<TaskFile>,
    pub settings: serde_json::Value,
    pub files: BTreeMap<PathBuf, CompiledFile>,
}

/// Materialized support files keep their bytes and executable permissions.
#[derive(Debug, Clone)]
pub struct CompiledFile {
    pub bytes: Vec<u8>,
    pub permissions: Option<std::fs::Permissions>,
}
impl From<Vec<u8>> for CompiledFile {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes, permissions: None }
    }
}

#[derive(Debug, Clone)]
pub struct TaskFile {
    pub path: PathBuf,
    pub tasks: Vec<Task>,
}

/// Discovery/input rendering is supplied by the front end. The core recursively
/// expands this tree, checks ownership, and emits one ordinary workspace.
#[derive(Debug, Clone)]
pub struct Block {
    pub name: String,
    pub source: PathBuf,
    pub version: String,
    pub manifest: BlockManifest,
    pub local: Option<Fragment>,
    pub children: Vec<(String, Block)>,
}

#[derive(Debug, Clone)]
pub struct CompiledBlock {
    pub fragment: Fragment,
    pub entry: String,
    pub exits: BTreeMap<String, String>,
    pub inputs: BTreeMap<String, Endpoint>,
    pub outputs: BTreeMap<String, Endpoint>,
    pub primary: Vec<String>,
    pub(crate) primary_profiles: BTreeSet<String>,
    pub(crate) flow_name: Option<String>,
    pub(crate) stable_primary_profiles: BTreeSet<String>,
    pub origins: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Endpoint {
    File { state: String, name: String, path: String, output: bool },
    Export { task: String, name: String },
}

impl Block {
    pub fn compile(self) -> CompileResult<CompiledBlock> {
        let mut compiled = self.compile_at(&[], &mut Vec::new())?;
        compiled.derive_routing()?;
        Ok(compiled)
    }

    fn compile_at(
        self,
        chain: &[String],
        stack: &mut Vec<PathBuf>,
    ) -> CompileResult<CompiledBlock> {
        let source = self.source.clone();
        if stack.contains(&source) {
            return Err(format!(
                "recursive block cycle: {:?} -> {}; break the use chain",
                stack,
                source.display()
            ));
        }
        stack.push(source.clone());
        let result = self.compile_inner(chain, stack);
        stack.pop();
        result.map_err(|e| format!("{} [mount {}]: {e}", source.display(), chain.join(".")))
    }

    fn compile_inner(
        self,
        chain: &[String],
        stack: &mut Vec<PathBuf>,
    ) -> CompileResult<CompiledBlock> {
        let mut children = BTreeMap::new();
        let aliases = self.children.iter().map(|(a, _)| a.clone()).collect::<Vec<_>>();
        for (alias, block) in self.children {
            if !valid_identifier(&alias) || children.contains_key(&alias) {
                return Err(format!(
                    "invalid or duplicate alias '{alias}'; choose distinct identifiers"
                ));
            }
            let mut child_chain = chain.to_vec();
            child_chain.push(alias.clone());
            let mut compiled = block.compile_at(&child_chain, stack)?;
            compiled.qualify(&Qualifier::new(vec![alias.clone()]))?;
            children.insert(alias, compiled);
        }
        let seams = self.manifest.seams.clone().unwrap_or_else(|| {
            aliases
                .windows(2)
                .map(|p| Seam {
                    from: format!("{}.done", p[0]),
                    to: format!("{}.entry", p[1]),
                    pass: BTreeMap::new(),
                })
                .collect()
        });
        let order = linear_seam_order(&aliases, &seams)?;
        let mut result = CompiledBlock::empty(&self.name);
        let local_present = self.local.is_some();
        if let Some(local) = self.local {
            result.fragment = local;
            result.validate_references().map_err(|e| {
                format!("{}: {e}", self.source.with_file_name("states.yaml").display())
            })?;
            result.primary = result
                .fragment
                .machine
                .node_policy
                .as_ref()
                .and_then(|p| result.fragment.machine.profiles.as_ref()?.get(&p.default))
                .map(|p| p.allowed.clone())
                .unwrap_or_else(|| result.fragment.machine.states.keys().cloned().collect());
            if let Some(policy) = &result.fragment.machine.node_policy {
                result.primary_profiles.insert(policy.default.clone());
            }
            result.inputs = result.resolve_data(&self.manifest.data.inputs, false)?;
            result.outputs = result.resolve_data(&self.manifest.data.outputs, true)?;
        } else if !self.manifest.data.inputs.is_empty() || !self.manifest.data.outputs.is_empty() {
            return Err("local data endpoints require a local state fragment or plan".into());
        }
        let ports = self.manifest.ports.as_ref();
        if ports.is_none() && (!chain.is_empty() || aliases.is_empty()) {
            return Err("mounted block is missing ports; declare ports.entry and terminal ports.exits (done for default sequencing)".into());
        }
        if let Some(ports) = ports {
            result.entry = control(&ports.entry, true, &result, &children)?;
            for (port, target) in &ports.exits {
                let state = control(target, false, &result, &children)?;
                result.exits.insert(port.clone(), state);
            }
        } else if let Some(first) = order.first() {
            result.entry = children[first].entry.clone();
            result.exits = children[order.last().unwrap()].exits.clone();
        }
        // Data endpoints retain their source owner while control spans siblings.
        let mut links = Vec::new();
        let mut passes = Vec::new();
        for seam in &seams {
            let from = control(&seam.from, false, &result, &children)?;
            let to = control(&seam.to, true, &result, &children)?;
            links.push((from, to));
            for (from, to) in &seam.pass {
                passes.push((data(from, true, &children)?, data(to, false, &children)?));
            }
        }
        let compatibility =
            super::compatibility::resolve(&self.manifest.compatibility, &children, &result)?;
        for alias in order {
            result.merge(children.remove(&alias).unwrap())?;
        }
        if !local_present && result.fragment.machine.states.is_empty() {
            return Err("block supplies no state fragment; add states.yaml or mount a block".into());
        }
        for (from, to) in &links {
            let state = result
                .fragment
                .machine
                .states
                .get_mut(from)
                .ok_or_else(|| format!("missing seam source {from}"))?;
            if !state.terminal {
                return Err(format!("seam source '{from}' is not terminal before composition"));
            }
            state.terminal = false;
            result.fragment.machine.transitions.push(crate::ast::TransitionRule {
                from: crate::ast::StateName(from.clone()),
                to: crate::ast::StateName(to.clone()),
                on_leave: None,
                on_enter: None,
                condition: None,
                timeout: None,
                exit_code: None,
                mcp_unavailable: None,
                skill_unavailable: None,
            });
        }
        result.extend_internal_routes(&links);
        for (source, target) in passes {
            result.lower_pass(source, target)?;
        }
        super::compatibility::apply(&mut result, compatibility)?;
        result.origins.insert(
            0,
            format!(
                "{} {} source={} alias={}",
                self.name,
                self.version,
                self.source.display(),
                chain.join(".")
            ),
        );
        result.fragment.plan.title = self.name.clone();
        result.fragment.plan.states = self.name.clone();
        result.fragment.machine.name = self.name;
        Ok(result)
    }
}

impl CompiledBlock {
    fn empty(name: &str) -> Self {
        Self {
            fragment: Fragment {
                machine: StateMachine {
                    name: name.into(),
                    version: 1.into(),
                    models: vec![],
                    prompt_templates: IndexMap::new(),
                    states: IndexMap::new(),
                    transitions: vec![],
                    profiles: None,
                    node_policy: None,
                    metrics: IndexMap::new(),
                },
                plan: Rhei {
                    title: name.into(),
                    states: name.into(),
                    states_declared: true,
                    structure: Structure { max_levels: 1, node_kinds: vec![] },
                    metadata: None,
                    content_sections: vec![],
                    tasks: vec![],
                },
                tasks: vec![],
                settings: serde_json::json!({}),
                files: BTreeMap::new(),
            },
            entry: String::new(),
            exits: BTreeMap::new(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            primary: vec![],
            primary_profiles: BTreeSet::new(),
            flow_name: None,
            stable_primary_profiles: BTreeSet::new(),
            origins: vec![],
        }
    }

    fn merge(&mut self, other: Self) -> CompileResult<()> {
        let into = &mut self.fragment;
        let mut from = other.fragment;
        merge_map(&mut into.machine.states, from.machine.states, "state")?;
        merge_map(&mut into.machine.prompt_templates, from.machine.prompt_templates, "prompt")?;
        merge_map(
            into.machine.profiles.get_or_insert_with(IndexMap::new),
            from.machine.profiles.take().unwrap_or_default(),
            "profile",
        )?;
        if let Some(policy) = from.machine.node_policy {
            let target = into.machine.node_policy.get_or_insert_with(|| NodePolicy {
                root: "flow".into(),
                default: "flow".into(),
                by_type: IndexMap::new(),
                overrides: vec![],
            });
            merge_map(&mut target.by_type, policy.by_type, "routing kind")?;
            target.overrides.extend(policy.overrides);
        }
        into.machine.transitions.extend(from.machine.transitions);
        for model in from.machine.models {
            if !into.machine.models.contains(&model) {
                into.machine.models.push(model);
            }
        }
        into.tasks.extend(from.tasks);
        into.plan.content_sections.extend(from.plan.content_sections);
        into.plan.structure.max_levels =
            into.plan.structure.max_levels.max(from.plan.structure.max_levels);
        for kind in from.plan.structure.node_kinds {
            if !into.plan.structure.node_kinds.contains(&kind) {
                into.plan.structure.node_kinds.push(kind);
            }
        }
        if let Some(metadata) = from.plan.metadata {
            let target = into.plan.metadata.get_or_insert_with(Default::default);
            for (key, value) in metadata {
                if key.as_str() == Some("structure") {
                    continue;
                }
                if target.get(&key).is_some_and(|existing| existing != &value) {
                    return Err(format!("conflicting plan metadata {key:?}"));
                }
                target.insert(key, value);
            }
        }
        super::settings::merge(&mut into.settings, from.settings)?;
        for (path, bytes) in from.files {
            if into.files.insert(path.clone(), bytes).is_some() {
                return Err(format!("private file collision at {}", path.display()));
            }
        }
        self.primary.extend(other.primary);
        self.primary_profiles.extend(other.primary_profiles);
        self.stable_primary_profiles.extend(other.stable_primary_profiles);
        self.origins.extend(other.origins);
        Ok(())
    }

    // Internal lanes that share a consumed exit follow its continuation;
    // their entry and internal routing remain owned. §FS-rhei-library.5
    fn extend_internal_routes(&mut self, links: &[(String, String)]) {
        let machine = &mut self.fragment.machine;
        for (from, to) in links {
            let mut pending = vec![to.clone()];
            let mut continuation = Vec::new();
            while let Some(state) = pending.pop() {
                if continuation.contains(&state) {
                    continue;
                }
                continuation.push(state.clone());
                if machine.states.get(&state).is_some_and(|s| s.terminal) {
                    continue;
                }
                pending.extend(
                    machine
                        .transitions
                        .iter()
                        .filter(|r| r.from.0 == state)
                        .map(|r| r.to.0.clone()),
                );
            }
            for (name, profile) in machine.profiles.iter_mut().flat_map(|p| p.iter_mut()) {
                if self.primary_profiles.contains(name) || !profile.allowed.contains(from) {
                    continue;
                }
                for state in &continuation {
                    if !profile.allowed.contains(state) {
                        profile.allowed.push(state.clone());
                    }
                }
            }
        }
    }

    /// Primary profiles fold into the outer lane; all legacy initial flags go.
    /// Internal profiles retain their own policy and reachability. §FS-rhei-library.5
    fn derive_routing(&mut self) -> CompileResult<()> {
        let machine = &mut self.fragment.machine;
        for state in machine.states.values_mut() {
            state.initial = false;
        }
        let profiles = machine.profiles.get_or_insert_with(IndexMap::new);
        let policy = machine.node_policy.get_or_insert_with(|| NodePolicy {
            root: "flow".into(),
            default: "flow".into(),
            by_type: IndexMap::new(),
            overrides: vec![],
        });
        let flow = self.flow_name.clone().unwrap_or_else(|| "flow".into());
        for primary in &self.primary_profiles {
            if self.stable_primary_profiles.contains(primary) && primary != &flow {
                if let Some(profile) = profiles.get_mut(primary) {
                    profile.allowed = self.primary.clone();
                }
            } else {
                profiles.shift_remove(primary);
            }
        }
        for profile in
            policy.by_type.values_mut().chain(policy.overrides.iter_mut().map(|r| &mut r.profile))
        {
            if self.primary_profiles.contains(profile) {
                *profile = flow.clone();
            }
        }
        if profiles.contains_key(&flow) {
            return Err(format!("derived profile '{flow}' collides with an internal profile"));
        }
        profiles.insert(
            flow.clone(),
            Profile { initial: self.entry.clone(), allowed: self.primary.clone() },
        );
        policy.root = flow.clone();
        policy.default = flow;
        Ok(())
    }
}

pub(crate) fn merge_map<T>(
    into: &mut IndexMap<String, T>,
    from: IndexMap<String, T>,
    kind: &str,
) -> CompileResult<()> {
    for (key, value) in from {
        if into.contains_key(&key) {
            return Err(format!("{kind} ownership collision '{key}'"));
        }
        into.insert(key, value);
    }
    Ok(())
}

fn control(
    value: &str,
    entry: bool,
    local: &CompiledBlock,
    children: &BTreeMap<String, CompiledBlock>,
) -> CompileResult<String> {
    if let Some((alias, port)) = split_endpoint(value) {
        let child = children
            .get(alias)
            .ok_or_else(|| format!("unknown child '{alias}' in control endpoint '{value}'"))?;
        if entry && port == "entry" {
            return Ok(child.entry.clone());
        }
        if !entry {
            return child.exits.get(port).cloned().ok_or_else(|| {
                format!("unknown control port '{value}'; public exits: {:?}", child.exits.keys())
            });
        }
        return Err(format!("control entry '{value}' must name {alias}.entry"));
    }
    let state = local.fragment.machine.states.get(value).ok_or_else(|| {
        format!("unknown local control state '{value}'; declare it in states.yaml")
    })?;
    if !entry && !state.terminal {
        return Err(format!("exit '{value}' must be terminal before composition"));
    }
    Ok(value.into())
}

fn data(
    value: &str,
    output: bool,
    children: &BTreeMap<String, CompiledBlock>,
) -> CompileResult<Endpoint> {
    let (alias, name) =
        split_endpoint(value).ok_or_else(|| format!("invalid data endpoint '{value}'"))?;
    let child = children.get(alias).ok_or_else(|| format!("unknown data mount '{alias}'"))?;
    let ports = if output { &child.outputs } else { &child.inputs };
    ports.get(name).cloned().ok_or_else(|| {
        format!("unknown data endpoint '{value}'; public endpoints: {:?}", ports.keys())
    })
}
