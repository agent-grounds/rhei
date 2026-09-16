//! Resolve endpoints against exact typed owners and lower existing handoffs.
//! §FS-rhei-library.1 §FS-rhei-library.6
use super::references::{tasks, tasks_mut};
use super::*;
use crate::ast::{ConsumedExport, TaskId, TaskIdSegment};

impl CompiledBlock {
    pub(crate) fn resolve_data(
        &self,
        ports: &BTreeMap<String, DataEndpoint>,
        output: bool,
    ) -> CompileResult<BTreeMap<String, Endpoint>> {
        ports.iter().map(|(public, port)| {
            let endpoint = match port.kind {
                DataKind::StateFile => {
                    let state = port.state.as_ref().filter(|_| port.task.is_none()).ok_or_else(|| format!("state-file endpoint '{public}' must name only a state"))?;
                    let def = self.fragment.machine.states.get(state).ok_or_else(|| format!("data endpoint '{public}' names missing state '{state}'"))?;
                    let artifacts = if output { &def.outputs } else { &def.inputs };
                    let artifact = artifacts.iter().find(|a| a.name == port.name).ok_or_else(|| format!("data endpoint '{public}' names missing {} artifact '{state}.{}'", if output { "output" } else { "input" }, port.name))?;
                    Endpoint::File { state: state.clone(), name: port.name.clone(), path: artifact.path.clone(), output }
                }
                DataKind::TaskExport => {
                    let task = port.task.as_ref().filter(|_| port.state.is_none()).ok_or_else(|| format!("task-export endpoint '{public}' must name only a task"))?;
                    let mut producer = None;
                    for file in &self.fragment.tasks { tasks(&file.tasks, &mut |t| { if t.id.to_string() == *task { producer = Some(t.provides.clone()); } }); }
                    let provides = producer.ok_or_else(|| format!("endpoint '{public}' names missing task '{task}'"))?;
                    if output && !provides.contains(&port.name) { return Err(format!("task-export endpoint '{public}' names missing export '{task}:{}'; this task provides {provides:?}", port.name)); }
                    Endpoint::Export { task: task.clone(), name: port.name.clone() }
                }
            };
            Ok((public.clone(), endpoint))
        }).collect()
    }

    pub(crate) fn lower_pass(&mut self, source: Endpoint, target: Endpoint) -> CompileResult<()> {
        match (source, target) {
            (Endpoint::File { path: source, .. }, Endpoint::File { path: target, .. }) => {
                // All declarations of this owned input path are the same file
                // contract, including downstream consumers. §FS-rhei-library.6
                for state in self.fragment.machine.states.values_mut() {
                    for input in &mut state.inputs {
                        if input.path == target {
                            input.path = source.clone();
                        }
                    }
                }
                for endpoint in self.inputs.values_mut().chain(self.outputs.values_mut()) {
                    if let Endpoint::File { path, .. } = endpoint {
                        if *path == target {
                            *path = source.clone();
                        }
                    }
                }
            }
            (
                Endpoint::Export { task: producer, name },
                Endpoint::Export { task: consumer, .. },
            ) => {
                let producer = TaskId::from_segments(
                    producer
                        .split('.')
                        .map(|s| {
                            s.parse()
                                .map(TaskIdSegment::Number)
                                .unwrap_or_else(|_| TaskIdSegment::Named(s.into()))
                        })
                        .collect(),
                );
                let mut found = false;
                for file in &mut self.fragment.tasks {
                    tasks_mut(&mut file.tasks, &mut |task| {
                        if task.id.to_string() == consumer {
                            let reference =
                                ConsumedExport { task: producer.clone(), name: name.clone() };
                            if !task.consumes.contains(&reference) {
                                task.consumes.push(reference);
                            }
                            found = true;
                        }
                    });
                }
                if !found {
                    return Err(format!("missing task-export consumer '{consumer}'"));
                }
            }
            _ => return Err(
                "incompatible data pass kinds: state-file and task-export; use matching endpoints"
                    .into(),
            ),
        }
        Ok(())
    }
}
