//! Checked identities are lowered before an outer mount qualifies the wrapper.
//! §FS-rhei-library.7 §AR-rhei-library.4
use super::references::{tasks, Names};
use super::*;

pub(crate) fn resolve(
    map: &CompatibilityMap,
    children: &BTreeMap<String, CompiledBlock>,
    local: &CompiledBlock,
) -> CompileResult<Names> {
    if let Some((a, b, target)) = map.collision() {
        return Err(format!("compatibility collision: '{a}' and '{b}' both claim '{target}'"));
    }
    let mut result = Names::default();
    for (kind, mappings, output) in [
        ("state", &map.states, &mut result.states),
        ("task", &map.tasks, &mut result.tasks),
        ("profile", &map.profiles, &mut result.profiles),
    ] {
        let mut all = owned(local, kind);
        for child in children.values() {
            all.extend(owned(child, kind));
        }
        for (stable, value) in mappings {
            let (alias, name) = value.split_once('.').ok_or_else(|| {
                format!("compatibility {kind} target '{value}' must be <alias>.<local-name>")
            })?;
            let child = children
                .get(alias)
                .ok_or_else(|| format!("compatibility target '{value}' names unknown child"))?;
            let target = Qualifier::new(vec![alias.into()]).qualify(name);
            if !owned(child, kind).contains(&target) {
                return Err(format!("unresolved compatibility {kind} target '{value}'; correct its identity in the child manifest"));
            }
            if all.contains(stable) && stable != &target {
                return Err(format!(
                    "compatibility {kind} '{stable}' collides with an existing identity"
                ));
            }
            output.insert(target, stable.clone());
        }
    }
    // Apply setting aliases only in the registries that own the target.
    // Same-spelled external references keep their identity. §FS-rhei-library.4
    for (stable, value) in &map.settings {
        let (alias, name) = value.split_once('.').ok_or_else(|| {
            format!("compatibility setting target '{value}' must be <alias>.<local-name>")
        })?;
        let child = children
            .get(alias)
            .ok_or_else(|| format!("compatibility target '{value}' names unknown child"))?;
        let target = Qualifier::new(vec![alias.into()]).qualify(name);
        let mut found = false;
        for (kind, output) in result.settings.registries_mut() {
            if !owned(child, kind).contains(&target) {
                continue;
            }
            found = true;
            if stable != &target
                && std::iter::once(local)
                    .chain(children.values())
                    .any(|block| owned(block, kind).contains(stable))
            {
                return Err(format!(
                    "compatibility {kind} '{stable}' collides with an existing identity"
                ));
            }
            output.insert(target.clone(), stable.clone());
        }
        if !found {
            return Err(format!("unresolved compatibility setting target '{value}'; correct its identity in the child manifest"));
        }
    }
    for (stable, value) in &map.artifacts {
        super::qualify::relative(stable)?;
        let (alias, endpoint) = split_endpoint(value)
            .ok_or_else(|| format!("invalid compatibility artifact '{value}'"))?;
        let child = children
            .get(alias)
            .ok_or_else(|| format!("unknown compatibility artifact mount '{alias}'"))?;
        let endpoint = child
            .outputs
            .get(endpoint)
            .or_else(|| child.inputs.get(endpoint))
            .ok_or_else(|| format!("unknown compatibility artifact endpoint '{value}'"))?;
        let Endpoint::File { path, .. } = endpoint else {
            return Err(format!("compatibility artifact '{value}' must be state-file"));
        };
        let collision = std::iter::once(local).chain(children.values()).any(|c| {
            c.fragment.machine.states.values().any(|s| {
                s.inputs.iter().chain(&s.outputs).any(|a| a.path == *stable && a.path != *path)
            })
        });
        if collision {
            return Err(format!(
                "compatibility artifact '{stable}' collides with another owned path"
            ));
        }
        if result.paths.insert(path.clone(), stable.clone()).is_some() {
            return Err(format!("compatibility artifact '{value}' is claimed twice"));
        }
    }
    super::terminal_equivalence::resolve(map, children, local, &mut result)?;
    Ok(result)
}

fn owned(block: &CompiledBlock, kind: &str) -> BTreeSet<String> {
    match kind {
        "state" => block.fragment.machine.states.keys().cloned().collect(),
        "profile" => {
            block.fragment.machine.profiles.iter().flat_map(|p| p.keys().cloned()).collect()
        }
        "agents" | "models" | "mcp_servers" | "skills" => block
            .fragment
            .settings
            .get(kind)
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flat_map(|m| m.keys().cloned())
            .collect(),
        "task" => {
            let mut result = BTreeSet::new();
            for f in &block.fragment.tasks {
                tasks(&f.tasks, &mut |t| {
                    result.insert(t.id.to_string());
                });
            }
            result
        }
        _ => BTreeSet::new(),
    }
}

pub(crate) fn apply(block: &mut CompiledBlock, names: Names) -> CompileResult<()> {
    // A public task restored at this boundary retains its authored task file.
    // Qualified files are otherwise private and remain collision-free.
    for file in &mut block.fragment.tasks {
        let mut restored = false;
        tasks(&file.tasks, &mut |task| {
            restored |= names.tasks.contains_key(&task.id.to_string());
        });
        if restored {
            if let Some(name) = file.path.file_name() {
                file.path = std::path::PathBuf::from("tasks").join(name);
            }
        }
    }
    for (source, target) in &names.profiles {
        if block.primary_profiles.contains(source) {
            if block.flow_name.as_ref().is_some_and(|name| name != target) {
                return Err(
                    "multiple stable primary profiles cannot name one derived outer lane".into()
                );
            }
            block.flow_name = Some(target.clone());
            block.stable_primary_profiles.insert(source.clone());
        }
    }
    block.rewrite(&names)
}
