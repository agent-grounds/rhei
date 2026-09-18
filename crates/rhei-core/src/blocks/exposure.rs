//! Author-owned public identity tables and boundary validation.
//! §FS-rhei-library.1.2 §AR-rhei-library.3–4

use super::references::{rename, tasks, Names};
use super::*;
use crate::state_machine::{parse_execution_target, StateMcpEntry, StateSkillEntry};

/// Public keys are kept separately from concrete compiled identities so every
/// mount boundary can enforce default privacy. §FS-rhei-library.1.2
#[derive(Debug, Clone, Default)]
pub(crate) struct PublicNames {
    pub states: BTreeMap<String, String>,
    pub tasks: BTreeMap<String, String>,
    pub settings: super::settings::SettingNames,
}

impl PublicNames {
    fn registry(&self, kind: &str) -> &BTreeMap<String, String> {
        match kind {
            "state" => &self.states,
            "task" => &self.tasks,
            "agents" => &self.settings.agents,
            "models" => &self.settings.models,
            "mcp_servers" => &self.settings.mcp_servers,
            "skills" => &self.settings.skills,
            _ => unreachable!("closed exposure kind"),
        }
    }

    fn registry_mut(&mut self, kind: &str) -> &mut BTreeMap<String, String> {
        match kind {
            "state" => &mut self.states,
            "task" => &mut self.tasks,
            "agents" => &mut self.settings.agents,
            "models" => &mut self.settings.models,
            "mcp_servers" => &mut self.settings.mcp_servers,
            "skills" => &mut self.settings.skills,
            _ => unreachable!("closed exposure kind"),
        }
    }

    pub(crate) fn rewrite_values(&mut self, names: &Names) {
        for value in self.states.values_mut() {
            rename(value, &names.states);
        }
        for value in self.tasks.values_mut() {
            rename(value, &names.tasks);
        }
        for (kind, values) in self.settings.registries_mut() {
            let replacements = match kind {
                "agents" => &names.settings.agents,
                "models" => &names.settings.models,
                "mcp_servers" => &names.settings.mcp_servers,
                "skills" => &names.settings.skills,
                _ => unreachable!("closed settings registry"),
            };
            for value in values.values_mut() {
                rename(value, replacements);
            }
        }
    }
}

/// Build the only names a parent may use across each immediate child boundary.
/// Private and generated spellings are deliberately absent. §FS-rhei-library.1.2
pub(crate) fn public_uses(children: &BTreeMap<String, CompiledBlock>) -> Names {
    let mut names = Names::default();
    for (alias, child) in children {
        for kind in ["state", "task", "agents", "models", "mcp_servers", "skills"] {
            for (public, concrete) in child.public.registry(kind) {
                let authored = format!("{alias}.{public}");
                names_registry_mut(&mut names, kind).insert(authored, concrete.clone());
            }
        }
    }
    names
}

fn names_registry_mut<'a>(names: &'a mut Names, kind: &str) -> &'a mut BTreeMap<String, String> {
    match kind {
        "state" => &mut names.states,
        "task" => &mut names.tasks,
        "agents" => &mut names.settings.agents,
        "models" => &mut names.settings.models,
        "mcp_servers" => &mut names.settings.mcp_servers,
        "skills" => &mut names.settings.skills,
        _ => unreachable!("closed exposure kind"),
    }
}

/// Reject a typed reference that tries to guess through a mount boundary, and
/// explain the immediate public alternatives. §FS-rhei-library.8
pub(crate) fn validate_uses(
    block: &CompiledBlock,
    children: &BTreeMap<String, CompiledBlock>,
) -> CompileResult<()> {
    let machine = &block.fragment.machine;
    for rule in &machine.transitions {
        if rule.from.0 != "*" {
            check(&rule.from.0, "state", children)?;
        }
        check(&rule.to.0, "state", children)?;
        check_yaml_ids(&rule.mcp_unavailable, "mcp_servers", children)?;
        check_yaml_ids(&rule.skill_unavailable, "skills", children)?;
    }
    for profile in machine.profiles.iter().flat_map(|profiles| profiles.values()) {
        check(&profile.initial, "state", children)?;
        for state in &profile.allowed {
            check(state, "state", children)?;
        }
    }
    for model in &machine.models {
        check(model, "models", children)?;
    }
    for state in machine.states.values() {
        for model in state.model.iter().chain(&state.all_models) {
            check(model, "models", children)?;
        }
        if let Some(agent) = &state.agent {
            check(&agent.0, "agents", children)?;
        }
        for selector in state.target.iter().chain(&state.all_targets) {
            check_target(selector, children)?;
        }
        if let Some(select) = state
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.inherit.as_ref())
            .and_then(|inherit| inherit.select.as_ref())
        {
            if let Some(state) = &select.state {
                check(state, "state", children)?;
            }
            if let Some(selector) = &select.target {
                check_target(selector, children)?;
            }
        }
        for entry in state.mcp_servers.iter().flatten() {
            check(
                match entry {
                    StateMcpEntry::Id(id) => id,
                    StateMcpEntry::Object(object) => &object.id,
                },
                "mcp_servers",
                children,
            )?;
        }
        for entry in state.skills.iter().flatten() {
            check(
                match entry {
                    StateSkillEntry::Id(id) => id,
                    StateSkillEntry::Object(object) => &object.id,
                },
                "skills",
                children,
            )?;
        }
    }
    let mut error = None;
    for file in &block.fragment.tasks {
        tasks(&file.tasks, &mut |task| {
            if error.is_some() {
                return;
            }
            let state = public_counted_base(&task.state, children).unwrap_or(&task.state);
            if let Err(found) = check(state, "state", children) {
                error = Some(found);
                return;
            }
            for prior in &task.prior {
                if let Err(found) = check(&prior.to_string(), "task", children) {
                    error = Some(found);
                    return;
                }
            }
            for consumed in &task.consumes {
                let producer = consumed.task.to_string();
                if let Err(found) = check(&producer, "task", children) {
                    error = Some(found);
                    return;
                }
                if is_public(&producer, "task", children) {
                    error = Some(format!(
                        "task exposure '{producer}' grants identity access only and does not expose export '{}'; declare a data endpoint and pass for that export",
                        consumed.name
                    ));
                    return;
                }
            }
            if let Some(model) = &task.model {
                if let Err(found) = check(model, "models", children) {
                    error = Some(found);
                    return;
                }
            }
            if let Some(selector) = &task.target {
                if let Err(found) = check_target(selector, children) {
                    error = Some(found);
                }
            }
        });
    }
    error.map_or(Ok(()), Err)
}

fn public_counted_base<'a>(
    value: &'a str,
    children: &BTreeMap<String, CompiledBlock>,
) -> Option<&'a str> {
    let (base, visit) = value.rsplit_once('-')?;
    visit.parse::<u32>().ok()?;
    is_public(base, "state", children).then_some(base)
}

fn check_yaml_ids(
    value: &Option<serde_yaml::Value>,
    kind: &str,
    children: &BTreeMap<String, CompiledBlock>,
) -> CompileResult<()> {
    if let Some(serde_yaml::Value::Sequence(values)) = value {
        for value in values {
            if let serde_yaml::Value::String(value) = value {
                check(value, kind, children)?;
            }
        }
    }
    Ok(())
}

fn check_target(value: &str, children: &BTreeMap<String, CompiledBlock>) -> CompileResult<()> {
    if let Ok(parsed) = parse_execution_target(value) {
        check(&parsed.agent, "agents", children)?;
        check(&parsed.model, "models", children)?;
    }
    Ok(())
}

fn is_public(value: &str, kind: &str, children: &BTreeMap<String, CompiledBlock>) -> bool {
    value
        .split_once('.')
        .and_then(|(alias, public)| children.get(alias).map(|child| (child, public)))
        .is_some_and(|(child, public)| {
            !public.contains('.') && child.public.registry(kind).contains_key(public)
        })
}

fn check(value: &str, kind: &str, children: &BTreeMap<String, CompiledBlock>) -> CompileResult<()> {
    let Some((alias, name)) = value.split_once('.') else { return Ok(()) };
    let Some(child) = children.get(alias) else { return Ok(()) };
    if !name.contains('.') && child.public.registry(kind).contains_key(name) {
        return Ok(());
    }
    let alternatives = child
        .public
        .registry(kind)
        .keys()
        .map(|public| format!("{alias}.{public}"))
        .collect::<Vec<_>>();
    let alternatives =
        if alternatives.is_empty() { "none declared".to_string() } else { alternatives.join(", ") };
    if name.contains('.') {
        return Err(format!(
            "'{value}' crosses more than the immediate mount boundary at '{}'; explicitly re-expose the {kind} in '{}' first; public alternatives: {alternatives}",
            child.source.display(),
            child.fragment.plan.title
        ));
    }
    for other in ["state", "task", "agents", "models", "mcp_servers", "skills"] {
        if other != kind && child.public.registry(other).contains_key(name) {
            return Err(format!(
                "public {other} '{value}' cannot be used as {kind}; public {kind} alternatives: {alternatives}"
            ));
        }
    }
    Err(format!(
        "private or unknown public {kind} '{value}' is not exposed; add an expose declaration in the child or use a declared public {kind}: {alternatives}"
    ))
}

pub(crate) struct ResolvedExposure {
    pub renames: Names,
    pub public: PublicNames,
}

/// Resolve local or immediate-child targets before qualification, keeping the
/// public key as the stable emitted identity. §FS-rhei-library.1.2 §FS-rhei-library.4
pub(crate) fn resolve(
    exposure: &Exposure,
    children: &BTreeMap<String, CompiledBlock>,
    local: &CompiledBlock,
) -> CompileResult<ResolvedExposure> {
    let mut renames = Names::default();
    let mut public = PublicNames::default();
    for (kind, declarations) in exposure_registries(exposure) {
        let all_owned = std::iter::once(local)
            .chain(children.values())
            .flat_map(|block| owned(block, kind))
            .collect::<BTreeSet<_>>();
        let mut claimed = BTreeMap::<String, String>::new();
        for (name, target) in declarations {
            if !valid_identifier(name) {
                return Err(format!(
                    "expose.{kind} public name '{name}' is invalid; use a letter then letters, digits, '_' or '-'"
                ));
            }
            let concrete = resolve_target(name, kind, target, children, local)?;
            if let Some(previous) = claimed.insert(concrete.clone(), name.clone()) {
                return Err(format!(
                    "expose {kind} public names '{previous}' and '{name}' both claim target '{concrete}'; expose it once"
                ));
            }
            if all_owned.contains(name) && name != &concrete {
                return Err(format!(
                    "expose {kind} public name '{name}' has a generated identity collision with an existing {kind}; rename the public key"
                ));
            }
            names_registry_mut(&mut renames, kind).insert(concrete, name.clone());
            public.registry_mut(kind).insert(name.clone(), name.clone());
        }
    }
    Ok(ResolvedExposure { renames, public })
}

fn exposure_registries(exposure: &Exposure) -> [(&str, &BTreeMap<String, ExposureTarget>); 6] {
    [
        ("state", &exposure.states),
        ("task", &exposure.tasks),
        ("agents", &exposure.settings.agents),
        ("models", &exposure.settings.models),
        ("mcp_servers", &exposure.settings.mcp_servers),
        ("skills", &exposure.settings.skills),
    ]
}

fn resolve_target(
    public: &str,
    kind: &str,
    target: &ExposureTarget,
    children: &BTreeMap<String, CompiledBlock>,
    local: &CompiledBlock,
) -> CompileResult<String> {
    match (&target.local, &target.mount, &target.name) {
        (Some(local_name), None, None) => {
            if owned(local, kind).contains(local_name) {
                Ok(local_name.clone())
            } else {
                Err(format!(
                    "expose {kind} '{public}' has unresolved local target '{local_name}' in {}; correct the local identity",
                    local.source.display()
                ))
            }
        }
        (None, Some(alias), Some(name)) => {
            let child = children.get(alias).ok_or_else(|| {
                format!(
                    "expose {kind} '{public}' names unknown immediate mount '{alias}'; correct the mount target"
                )
            })?;
            child.public.registry(kind).get(name).cloned().ok_or_else(|| {
                let alternatives = child
                    .public
                    .registry(kind)
                    .keys()
                    .map(|name| format!("{alias}.{name}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "expose {kind} '{public}' has unresolved child target '{alias}.{name}'; use a public {kind} from {}: {alternatives}",
                    child.source.display()
                )
            })
        }
        _ => Err(format!(
            "expose.{kind}.{public} must use exactly one target form: local alone, or mount together with name"
        )),
    }
}

fn owned(block: &CompiledBlock, kind: &str) -> BTreeSet<String> {
    match kind {
        "state" => block.fragment.machine.states.keys().cloned().collect(),
        "task" => {
            let mut result = BTreeSet::new();
            for file in &block.fragment.tasks {
                tasks(&file.tasks, &mut |task| {
                    result.insert(task.id.to_string());
                });
            }
            result
        }
        "agents" | "models" | "mcp_servers" | "skills" => block
            .fragment
            .settings
            .get(kind)
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flat_map(|values| values.keys().cloned())
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// Detached seam/pass records are resolved before exposure rewrites the merged
/// typed graph, so carry the same typed rename into them. §FS-rhei-library.4
pub(crate) fn rewrite_links(
    links: &mut [(String, String)],
    passes: &mut [(Endpoint, Endpoint, String)],
    names: &Names,
) {
    for (from, to) in links {
        rename(from, &names.states);
        rename(to, &names.states);
    }
    for (source, target_endpoint, _) in passes {
        rewrite_endpoint(source, names);
        rewrite_endpoint(target_endpoint, names);
    }
}

fn rewrite_endpoint(endpoint: &mut Endpoint, names: &Names) {
    match endpoint {
        Endpoint::File { state, .. } => rename(state, &names.states),
        Endpoint::Export { task, .. } => rename(task, &names.tasks),
    }
}

/// Keep a compatibility source keyed to the identity produced by exposure.
/// §FS-rhei-library.7
pub(crate) fn retarget_compatibility(compatibility: &mut Names, exposure: &Names) {
    retarget_map(&mut compatibility.states, &exposure.states);
    retarget_map(&mut compatibility.tasks, &exposure.tasks);
    for (kind, map) in compatibility.settings.registries_mut() {
        let renames = match kind {
            "agents" => &exposure.settings.agents,
            "models" => &exposure.settings.models,
            "mcp_servers" => &exposure.settings.mcp_servers,
            "skills" => &exposure.settings.skills,
            _ => unreachable!("closed settings registry"),
        };
        retarget_map(map, renames);
    }
}

fn retarget_map(map: &mut BTreeMap<String, String>, renames: &BTreeMap<String, String>) {
    let original = std::mem::take(map);
    for (mut source, target_name) in original {
        rename(&mut source, renames);
        map.insert(source, target_name);
    }
}
