//! Endpoint resolution retains source manifests and full mount chains. §FS-rhei-library.8
use super::*;

pub(crate) fn control(
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
                format!(
                    "unknown control port {}; public exits: {}",
                    endpoint_context(value, children),
                    child
                        .exits
                        .keys()
                        .map(|port| format!("{alias}.{port}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
        }
        return Err(format!(
            "control entry {} must name {alias}.entry",
            endpoint_context(value, children)
        ));
    }
    let state = local.fragment.machine.states.get(value).ok_or_else(|| {
        format!("unknown local control state '{value}'; declare it in states.yaml")
    })?;
    if !entry && !state.terminal {
        return Err(format!("exit '{value}' must be terminal before composition"));
    }
    Ok(value.into())
}

pub(crate) fn data(
    value: &str,
    output: bool,
    children: &BTreeMap<String, CompiledBlock>,
) -> CompileResult<Endpoint> {
    let (alias, name) =
        split_endpoint(value).ok_or_else(|| format!("invalid data endpoint '{value}'"))?;
    let child = children.get(alias).ok_or_else(|| format!("unknown data mount '{alias}'"))?;
    let ports = if output { &child.outputs } else { &child.inputs };
    ports.get(name).cloned().ok_or_else(|| {
        format!(
            "unknown data endpoint {}; public endpoints: {}",
            endpoint_context(value, children),
            ports.keys().map(|port| format!("{alias}.{port}")).collect::<Vec<_>>().join(", ")
        )
    })
}

/// Keep source context before sibling fragments are merged and lowered.
pub(crate) fn endpoint_context(value: &str, children: &BTreeMap<String, CompiledBlock>) -> String {
    match split_endpoint(value).and_then(|(alias, _)| children.get(alias)) {
        Some(child) => {
            format!("'{value}' in {} [mount {}]", child.source.display(), child.chain.join("."))
        }
        None => format!(
            "'{value}' (available mounts: {})",
            children.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
    }
}
