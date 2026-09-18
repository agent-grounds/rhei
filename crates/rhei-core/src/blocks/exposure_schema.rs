//! Preserve typed exposure paths when a manifest flattens block declarations.
//! Static and selected decoding share this closed schema. §FS-rhei-library.8
use super::{Exposure, ExposureTarget, SettingsExposure};
use serde::{de::Error, Deserialize, Deserializer};
use serde_yaml::{Mapping, Value};
use std::collections::BTreeMap;

impl<'de> Deserialize<'de> for Exposure {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        decode(&raw).map_err(D::Error::custom)
    }
}

fn decode(raw: &Value) -> Result<Exposure, String> {
    fields(raw, "expose", &["states", "tasks", "settings"])?;
    let mut settings = SettingsExposure::default();
    if let Some(raw) = raw.get("settings") {
        fields(raw, "expose.settings", &["agents", "models", "mcp_servers", "skills"])?;
        settings = SettingsExposure {
            agents: targets(raw.get("agents"), "expose.settings.agents")?,
            models: targets(raw.get("models"), "expose.settings.models")?,
            mcp_servers: targets(raw.get("mcp_servers"), "expose.settings.mcp_servers")?,
            skills: targets(raw.get("skills"), "expose.settings.skills")?,
        };
    }
    Ok(Exposure {
        states: targets(raw.get("states"), "expose.states")?,
        tasks: targets(raw.get("tasks"), "expose.tasks")?,
        settings,
    })
}

fn mapping<'a>(raw: &'a Value, path: &str) -> Result<&'a Mapping, String> {
    raw.as_mapping()
        .ok_or_else(|| format!("{path} must be a mapping; declare named exposure fields"))
}

fn fields(raw: &Value, path: &str, allowed: &[&str]) -> Result<(), String> {
    for key in mapping(raw, path)?.keys() {
        if !key.as_str().is_some_and(|key| allowed.contains(&key)) {
            return Err(format!(
                "unsupported field {key:?} in {path}; remove it or use an allowed field: {}",
                allowed.join(", ")
            ));
        }
    }
    Ok(())
}

fn targets(raw: Option<&Value>, path: &str) -> Result<BTreeMap<String, ExposureTarget>, String> {
    let Some(raw) = raw else { return Ok(BTreeMap::new()) };
    mapping(raw, path)?
        .iter()
        .map(|(name, target)| {
            let name = name.as_str().ok_or_else(|| {
                format!("invalid public name {name:?} in {path}; use a string identifier")
            })?;
            let path = format!("{path}.{name}");
            fields(target, &path, &["local", "mount", "name"])?;
            let target = serde_yaml::from_value(target.clone()).map_err(|error| {
                format!(
                    "{path}: {error}; use string identities in local, or mount together with name"
                )
            })?;
            Ok((name.into(), target))
        })
        .collect()
}
