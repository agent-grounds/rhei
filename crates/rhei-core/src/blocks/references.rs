//! Typed reference traversal, shared by qualification and compatibility.
//! §FS-rhei-library.4 §AR-rhei-library.4
use super::*;
use crate::ast::{Task, TaskId, TaskIdSegment};
use crate::state_machine::{
    parse_execution_target, parse_task_state, StateMcpEntry, StatePromptTemplateRef,
    StateSkillEntry,
};

#[derive(Default)]
pub(crate) struct Names {
    pub states: BTreeMap<String, String>,
    pub tasks: BTreeMap<String, String>,
    pub kinds: BTreeMap<String, String>,
    pub profiles: BTreeMap<String, String>,
    pub settings: super::settings::SettingNames,
    pub exports: BTreeMap<String, String>,
    pub prompts: BTreeMap<String, String>,
    pub paths: BTreeMap<String, String>,
}

pub(crate) fn rename(value: &mut String, names: &BTreeMap<String, String>) {
    if let Some(to) = names.get(value) {
        *value = to.clone();
    }
}
pub(crate) fn target(value: &mut String, names: &super::settings::SettingNames) {
    if let Ok(mut parsed) = parse_execution_target(value) {
        rename(&mut parsed.agent, &names.agents);
        rename(&mut parsed.model, &names.models);
        *value = parsed.selector();
    }
}
pub(crate) fn id(value: &mut TaskId, names: &BTreeMap<String, String>) {
    if let Some(to) = names.get(&value.to_string()) {
        value.segments = to
            .split('.')
            .map(|s| {
                s.parse()
                    .map(TaskIdSegment::Number)
                    .unwrap_or_else(|_| TaskIdSegment::Named(s.into()))
            })
            .collect();
    }
}
pub(crate) fn tasks_mut(tasks: &mut [Task], f: &mut impl FnMut(&mut Task)) {
    for task in tasks {
        f(task);
        tasks_mut(&mut task.children, f);
    }
}
pub(crate) fn tasks(tasks: &[Task], f: &mut impl FnMut(&Task)) {
    for task in tasks {
        f(task);
        self::tasks(&task.children, f);
    }
}

impl CompiledBlock {
    pub(crate) fn rewrite(&mut self, names: &Names) -> CompileResult<()> {
        let machine = &mut self.fragment.machine;
        for state in machine.states.values_mut() {
            for artifact in state.inputs.iter_mut().chain(&mut state.outputs) {
                rename(&mut artifact.path, &names.paths);
            }
            if let Some(prompt) = &mut state.prompt_template {
                match prompt {
                    StatePromptTemplateRef::Name(n)
                    | StatePromptTemplateRef::WithValues { name: n, .. } => {
                        rename(n, &names.prompts)
                    }
                }
            }
            for model in state.model.iter_mut().chain(&mut state.all_models) {
                rename(model, &names.settings.models);
            }
            if let Some(agent) = &mut state.agent {
                rename(&mut agent.0, &names.settings.agents);
            }
            for selector in state.target.iter_mut().chain(&mut state.all_targets) {
                target(selector, &names.settings);
            }
            if let Some(snapshot) = &mut state.snapshot {
                if let Some(select) = snapshot.inherit.as_mut().and_then(|i| i.select.as_mut()) {
                    if let Some(state) = &mut select.state {
                        rename(state, &names.states);
                    }
                    if let Some(t) = &mut select.target {
                        target(t, &names.settings);
                    }
                }
            }
            if let Some(entries) = &mut state.mcp_servers {
                for entry in entries {
                    match entry {
                        StateMcpEntry::Id(id) => rename(id, &names.settings.mcp_servers),
                        StateMcpEntry::Object(obj) => {
                            rename(&mut obj.id, &names.settings.mcp_servers);
                            if let Some(p) = &mut obj.working_directory {
                                rename(p, &names.paths);
                            }
                        }
                    }
                }
            }
            if let Some(entries) = &mut state.skills {
                for entry in entries {
                    match entry {
                        StateSkillEntry::Id(id) => rename(id, &names.settings.skills),
                        StateSkillEntry::Object(obj) => {
                            rename(&mut obj.id, &names.settings.skills);
                            if let Some(p) = &mut obj.path {
                                rename(p, &names.paths);
                            }
                        }
                    }
                }
            }
            // `program` is deliberately a Value in the shared runtime schema.
            if let Some(map) = state.program.as_mut().and_then(serde_yaml::Value::as_mapping_mut) {
                if let Some(serde_yaml::Value::String(path)) = map.get_mut("working_directory") {
                    rename(path, &names.paths);
                }
            }
        }
        rename_keys(&mut machine.prompt_templates, &names.prompts)?;
        for rule in &mut machine.transitions {
            rename(&mut rule.from.0, &names.states);
            rename(&mut rule.to.0, &names.states);
            for source in rule.sources.iter_mut().flatten() {
                rename(source, &names.states);
            }
            for (field, registry) in [
                (&mut rule.mcp_unavailable, &names.settings.mcp_servers),
                (&mut rule.skill_unavailable, &names.settings.skills),
            ] {
                if let Some(serde_yaml::Value::Sequence(ids)) = field {
                    for id in ids {
                        if let serde_yaml::Value::String(id) = id {
                            rename(id, registry);
                        }
                    }
                }
            }
        }
        for model in &mut machine.models {
            rename(model, &names.settings.models);
        }
        if let Some(profiles) = &mut machine.profiles {
            for profile in profiles.values_mut() {
                rename(&mut profile.initial, &names.states);
                for s in &mut profile.allowed {
                    rename(s, &names.states);
                }
            }
            rename_keys(profiles, &names.profiles)?;
        }
        if let Some(policy) = &mut machine.node_policy {
            rename(&mut policy.root, &names.profiles);
            rename(&mut policy.default, &names.profiles);
            rename_keys(&mut policy.by_type, &names.kinds)?;
            for p in policy.by_type.values_mut() {
                rename(p, &names.profiles);
            }
            for ov in &mut policy.overrides {
                if let Some(k) = &mut ov.match_.node_type {
                    rename(k, &names.kinds);
                }
                rename(&mut ov.profile, &names.profiles);
            }
        }
        for file in &mut self.fragment.tasks {
            tasks_mut(&mut file.tasks, &mut |task| {
                id(&mut task.id, &names.tasks);
                rename(&mut task.kind, &names.kinds);
                // Parse before renaming definitions: exact names beat visit suffixes.
                // §FS-rhei-library.4
                let parsed = parse_task_state(&task.state, machine);
                if let Some(state) = names.states.get(&parsed.state) {
                    task.state = match parsed.visit {
                        Some(visit) => format!("{state}-{visit}"),
                        None => state.clone(),
                    };
                }
                for prior in &mut task.prior {
                    id(prior, &names.tasks);
                }
                for kind in task.prior_kinds.iter_mut().flatten() {
                    if let Some(qualified) = names.kinds.get(&kind.to_lowercase()) {
                        *kind = qualified.clone();
                    }
                }
                for export in &mut task.provides {
                    rename(export, &names.exports);
                }
                for export in &mut task.consumes {
                    id(&mut export.task, &names.tasks);
                    rename(&mut export.name, &names.exports);
                }
                if let Some(model) = &mut task.model {
                    rename(model, &names.settings.models);
                }
                if let Some(t) = &mut task.target {
                    target(t, &names.settings);
                }
            });
        }
        rename_keys(&mut machine.states, &names.states)?;
        for kind in &mut self.fragment.plan.structure.node_kinds {
            rename(kind, &names.kinds);
        }
        self.primary_profiles = self
            .primary_profiles
            .iter()
            .map(|p| names.profiles.get(p).cloned().unwrap_or_else(|| p.clone()))
            .collect();
        self.stable_primary_profiles = self
            .stable_primary_profiles
            .iter()
            .map(|p| names.profiles.get(p).cloned().unwrap_or_else(|| p.clone()))
            .collect();
        if let Some(flow) = &mut self.flow_name {
            rename(flow, &names.profiles);
        }
        rename(&mut self.entry, &names.states);
        for state in self.exits.values_mut().chain(&mut self.primary) {
            rename(state, &names.states);
        }
        for endpoint in self.inputs.values_mut().chain(self.outputs.values_mut()) {
            match endpoint {
                Endpoint::File { state, path, .. } => {
                    rename(state, &names.states);
                    rename(path, &names.paths);
                }
                Endpoint::Export { task, name } => {
                    rename(task, &names.tasks);
                    rename(name, &names.exports);
                }
            }
        }
        super::settings::rewrite(&mut self.fragment.settings, &names.settings, &names.paths)?;
        Ok(())
    }
}

pub(crate) fn rename_keys<T>(
    map: &mut indexmap::IndexMap<String, T>,
    names: &BTreeMap<String, String>,
) -> CompileResult<()> {
    let original = std::mem::take(map);
    for (mut key, value) in original {
        rename(&mut key, names);
        if map.insert(key.clone(), value).is_some() {
            return Err(format!("identity collision '{key}'"));
        }
    }
    Ok(())
}
