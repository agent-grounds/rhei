//! Settings registry ownership without rewriting arbitrary strings.
//! §FS-rhei-library.4 §AR-rhei-library.4
use super::references::rename;
use super::*;
use serde_json::Value;

/// Registry kinds are separate ownership domains. §FS-rhei-library.4
#[derive(Debug, Clone, Default)]
pub(crate) struct SettingNames {
    pub agents: BTreeMap<String, String>,
    pub models: BTreeMap<String, String>,
    pub mcp_servers: BTreeMap<String, String>,
    pub skills: BTreeMap<String, String>,
}

impl SettingNames {
    pub fn registries(&self) -> [(&str, &BTreeMap<String, String>); 4] {
        [
            ("agents", &self.agents),
            ("models", &self.models),
            ("mcp_servers", &self.mcp_servers),
            ("skills", &self.skills),
        ]
    }

    pub fn registries_mut(&mut self) -> [(&str, &mut BTreeMap<String, String>); 4] {
        [
            ("agents", &mut self.agents),
            ("models", &mut self.models),
            ("mcp_servers", &mut self.mcp_servers),
            ("skills", &mut self.skills),
        ]
    }
}

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
    ids: &SettingNames,
    paths: &BTreeMap<String, String>,
) -> CompileResult<()> {
    for (section, owned) in ids.registries() {
        let Some(map) = settings.get_mut(section).and_then(Value::as_object_mut) else { continue };
        let original = std::mem::take(map);
        for (mut name, mut definition) in original {
            rename(&mut name, owned);
            if section == "models" {
                scalar(&mut definition, "default_agent", &ids.agents);
                if let Some(agents) = definition.get_mut("agents").and_then(Value::as_object_mut) {
                    let original = std::mem::take(agents);
                    for (mut id, binding) in original {
                        rename(&mut id, &ids.agents);
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
        scalar(defaults, "agent", &ids.agents);
        scalar(defaults, "model", &ids.models);
        for (field, owned) in [("mcp_servers", &ids.mcp_servers), ("skills", &ids.skills)] {
            if let Some(values) = defaults.get_mut(field).and_then(Value::as_array_mut) {
                for value in values {
                    if let Value::String(s) = value {
                        rename(s, owned);
                    } else {
                        scalar(value, "id", owned);
                    }
                }
            }
        }
    }
    Ok(())
}
