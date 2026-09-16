//! Qualification operates on definitions and typed references only.
//! §FS-rhei-library.4–5
use super::references::{tasks, Names};
use super::*;
use crate::state_machine::{parse_task_state, NodePolicyOverride};
use std::path::{Path, PathBuf};

pub(crate) fn relative(path: &str) -> CompileResult<String> {
    let path = path.replace('\\', "/");
    if crate::platform::path_is_rooted(&path)
        || path.contains(':')
        || Path::new(&path).components().any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("mounted path '{path}' must be relative and contain no '..'"));
    }
    Ok(Path::new(&path)
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

impl CompiledBlock {
    pub(crate) fn qualify(&mut self, q: &Qualifier) -> CompileResult<()> {
        if q.chain().is_empty() {
            return Ok(());
        }
        let mut names = Names::default();
        let machine = &mut self.fragment.machine;
        // Expand only authored nonterminal sources, after exact edges. This
        // preserves owner boundaries and leaves consumed exits for seams.
        // Reserved cancellation identity still requires the contract decision
        // recorded for R1-03; no runtime predicate is changed here.
        let rules = std::mem::take(&mut machine.transitions);
        machine.transitions.extend(rules.iter().filter(|r| r.from.0 != "*").cloned());
        for rule in rules.iter().filter(|r| r.from.0 == "*") {
            for (source, state) in &machine.states {
                if state.terminal || source == &rule.to.0 {
                    continue;
                }
                let mut exact = rule.clone();
                exact.from.0 = source.clone();
                machine.transitions.push(exact);
            }
        }
        for name in machine.states.keys() {
            names.states.insert(name.clone(), q.qualify(name));
        }
        let owns_task_kind = machine.node_policy.as_ref().is_some_and(|p| {
            p.by_type.keys().any(|k| k.eq_ignore_ascii_case("task"))
                || p.overrides.iter().any(|r| {
                    r.match_.node_type.as_deref().is_none_or(|k| k.eq_ignore_ascii_case("task"))
                })
        });
        for name in &self.fragment.plan.structure.node_kinds {
            if name != "task" || owns_task_kind {
                names.kinds.insert(name.clone(), q.qualify(name));
            }
        }
        for file in &self.fragment.tasks {
            tasks(&file.tasks, &mut |task| {
                names.tasks.insert(task.id.to_string(), q.qualify(&task.id.to_string()));
                for export in &task.provides {
                    names.exports.insert(export.clone(), q.qualify(export));
                }
            });
        }
        for name in machine.profiles.iter().flat_map(|p| p.keys()) {
            names.profiles.insert(name.clone(), q.qualify(name));
        }
        for (section, owned) in names.settings.registries_mut() {
            if let Some(ids) =
                self.fragment.settings.get(section).and_then(serde_json::Value::as_object)
            {
                for name in ids.keys() {
                    owned.insert(name.clone(), q.qualify(name));
                }
            }
        }
        for state in machine.states.values() {
            if let Some(prompt) = &state.prompt_template {
                names.prompts.insert(prompt.name().into(), q.qualify(prompt.name()));
            }
            for artifact in state.inputs.iter().chain(&state.outputs) {
                names.paths.insert(artifact.path.clone(), runtime_path(q, &artifact.path)?);
            }
            if let Some(path) = state
                .program
                .as_ref()
                .and_then(|v| v.get("working_directory"))
                .and_then(|v| v.as_str())
            {
                names.paths.insert(path.into(), private_path(q, path)?);
            }
            for entry in state.mcp_servers.iter().flatten() {
                if let crate::state_machine::StateMcpEntry::Object(obj) = entry {
                    if let Some(path) = &obj.working_directory {
                        names.paths.insert(path.clone(), private_path(q, path)?);
                    }
                }
            }
            for entry in state.skills.iter().flatten() {
                if let crate::state_machine::StateSkillEntry::Object(obj) = entry {
                    if let Some(path) = &obj.path {
                        names.paths.insert(path.clone(), private_path(q, path)?);
                    }
                }
            }
        }
        for name in machine.prompt_templates.keys() {
            names.prompts.insert(name.clone(), q.qualify(name));
        }
        super::settings::paths(&self.fragment.settings, q, &mut names.paths)?;
        // A level-only rule becomes one typed rule per owned kind. §FS-rhei-library.5
        if let Some(policy) = &mut machine.node_policy {
            let mut overrides = Vec::new();
            for rule in std::mem::take(&mut policy.overrides) {
                if rule.match_.node_type.is_some() {
                    overrides.push(rule);
                } else {
                    for kind in names.kinds.keys() {
                        overrides.push(NodePolicyOverride {
                            match_: crate::state_machine::NodePolicyMatch {
                                node_type: Some(kind.clone()),
                                level: rule.match_.level,
                            },
                            profile: rule.profile.clone(),
                        });
                    }
                }
            }
            policy.overrides = overrides;
        }
        self.rewrite(&names)?;
        for file in &mut self.fragment.tasks {
            let local = relative(&file.path.to_string_lossy())?;
            file.path = PathBuf::from("tasks")
                .join(q.prefix())
                .join(local.strip_prefix("tasks/").unwrap_or(&local));
        }
        let original = std::mem::take(&mut self.fragment.files);
        for (path, bytes) in original {
            let path = path.to_string_lossy();
            let target = if let Some(prompt) = path.strip_prefix("prompt_templates/") {
                PathBuf::from("prompt_templates").join(q.qualify(prompt))
            } else {
                PathBuf::from(private_path(q, &path)?)
            };
            if self.fragment.files.insert(target.clone(), bytes).is_some() {
                return Err(format!("private file collision {}", target.display()));
            }
        }
        Ok(())
    }

    pub(crate) fn validate_references(&self) -> CompileResult<()> {
        let m = &self.fragment.machine;
        let check = |state: &str| {
            if m.states.contains_key(state) {
                Ok(())
            } else {
                Err(format!("states.yaml names missing state '{state}'; fix the owned reference"))
            }
        };
        for rule in &m.transitions {
            if rule.from.0 != "*" {
                check(&rule.from.0)?;
            }
            check(&rule.to.0)?;
        }
        for profile in m.profiles.iter().flat_map(|p| p.values()) {
            check(&profile.initial)?;
            for state in &profile.allowed {
                check(state)?;
            }
        }
        for state in m.states.values() {
            if let Some(selected) = state
                .snapshot
                .as_ref()
                .and_then(|s| s.inherit.as_ref())
                .and_then(|i| i.select.as_ref())
                .and_then(|s| s.state.as_ref())
            {
                check(selected)?;
            }
        }
        let mut ids = BTreeSet::new();
        let mut duplicate = None;
        for file in &self.fragment.tasks {
            tasks(&file.tasks, &mut |t| {
                if !ids.insert(t.id.to_string()) {
                    duplicate = Some(t.id.to_string());
                }
            });
        }
        if let Some(id) = duplicate {
            return Err(format!("duplicate owned task '{id}'"));
        }
        let mut error = None;
        for file in &self.fragment.tasks {
            tasks(&file.tasks, &mut |t| {
                if let Err(e) = check(&parse_task_state(&t.state, m).state) {
                    error = Some(e);
                }
                for prior in &t.prior {
                    if !ids.contains(&prior.to_string()) {
                        error = Some(format!("missing owned Prior '{prior}' in task '{}'", t.id));
                    }
                }
            });
        }
        error.map_or(Ok(()), Err)
    }
}

// Requalifying a compiled wrapper extends the encoded chain rather than
// nesting an entire generated directory under another private directory.
fn runtime_path(q: &Qualifier, path: &str) -> CompileResult<String> {
    let path = relative(path)?;
    if let Some(rest) = path.strip_prefix("runtime/blocks/") {
        Ok(format!("runtime/blocks/{}{rest}", q.prefix()))
    } else {
        Ok(format!("runtime/blocks/{}/{path}", q.prefix()))
    }
}
pub(crate) fn private_path(q: &Qualifier, path: &str) -> CompileResult<String> {
    let path = relative(path)?;
    if let Some(rest) = path.strip_prefix(".agent-grounds/rhei/blocks/") {
        Ok(format!(".agent-grounds/rhei/blocks/{}{rest}", q.prefix()))
    } else {
        Ok(format!(".agent-grounds/rhei/blocks/{}/{path}", q.prefix()))
    }
}
