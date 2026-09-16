//! Settings registry ownership without rewriting arbitrary strings.
//! §FS-rhei-library.4 §AR-rhei-library.4
use super::references::rename;
use super::*;
use serde_json::Value;

pub(crate) fn merge(into: &mut Value, from: Value) -> CompileResult<()> {
    let target = into.as_object_mut().ok_or("settings must be an object")?;
    let source = from.as_object().ok_or("settings must be an object")?;
    for (section, value) in source {
        if let Some(existing) = target.get_mut(section) {
            if let (Some(dst), Some(src)) = (existing.as_object_mut(), value.as_object()) {
                for (key, value) in src {
                    if dst.contains_key(key) {
                        return Err(format!("settings ownership collision '{section}.{key}'"));
                    }
                    dst.insert(key.clone(), value.clone());
                }
            } else if existing != value {
                return Err(format!("conflicting settings '{section}'"));
            }
        } else {
            target.insert(section.clone(), value.clone());
        }
    }
    Ok(())
}

pub(crate) fn paths(
    settings: &Value,
    q: &Qualifier,
    paths: &mut BTreeMap<String, String>,
) -> CompileResult<()> {
    for (section, field) in [("mcp_servers", "working_directory"), ("skills", "path")] {
        for entry in
            settings.get(section).and_then(Value::as_object).into_iter().flat_map(|m| m.values())
        {
            if let Some(path) = entry.get(field).and_then(Value::as_str) {
                paths.insert(path.into(), super::qualify::private_path(q, path)?);
            }
        }
    }
    Ok(())
}

fn scalar(value: &mut Value, key: &str, names: &BTreeMap<String, String>) {
    if let Some(Value::String(s)) = value.get_mut(key) {
        rename(s, names);
    }
}

pub(crate) fn rewrite(
    settings: &mut Value,
    ids: &BTreeMap<String, String>,
    paths: &BTreeMap<String, String>,
) -> CompileResult<()> {
    for section in ["agents", "models", "mcp_servers", "skills"] {
        let Some(map) = settings.get_mut(section).and_then(Value::as_object_mut) else { continue };
        let original = std::mem::take(map);
        for (mut name, mut definition) in original {
            rename(&mut name, ids);
            if section == "models" {
                scalar(&mut definition, "default_agent", ids);
                if let Some(agents) = definition.get_mut("agents").and_then(Value::as_object_mut) {
                    let original = std::mem::take(agents);
                    for (mut id, binding) in original {
                        rename(&mut id, ids);
                        agents.insert(id, binding);
                    }
                }
            }
            if section == "mcp_servers" {
                scalar(&mut definition, "working_directory", paths);
            }
            if section == "skills" {
                scalar(&mut definition, "path", paths);
            }
            if map.insert(name.clone(), definition).is_some() {
                return Err(format!("settings collision '{section}.{name}'"));
            }
        }
    }
    if let Some(defaults) = settings.get_mut("defaults") {
        for field in ["agent", "model"] {
            scalar(defaults, field, ids);
        }
        for field in ["mcp_servers", "skills"] {
            if let Some(values) = defaults.get_mut(field).and_then(Value::as_array_mut) {
                for value in values {
                    if let Value::String(s) = value {
                        rename(s, ids);
                    } else {
                        scalar(value, "id", ids);
                    }
                }
            }
        }
    }
    Ok(())
}
