    // Plan qualification rewrites parsed task-language fields rather than
    // searching arbitrary Markdown prose. §FS-rhei-library.4
    fn qualify_tasks(
        rendered: &Path,
        layout: TemplateLayout,
        qualifier: &Qualifier,
        ownership: &SettingsOwnership,
    ) -> MietteResult<(Vec<(PathBuf, String)>, BTreeSet<String>)> {
        let mut sources = Vec::new();
        match layout {
            TemplateLayout::Workspace => {
                let tasks = rendered.join("tasks");
                collect_markdown_files(&tasks, &mut sources)?;
            }
            TemplateLayout::SingleFile => sources.push(rendered.join("plan.rhei.md")),
        }
        let heading = Regex::new(
            r"^(#{3,6})\s+([A-Za-z][A-Za-z0-9_-]*)\s+([A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)*):(.*)$",
        )
        .expect("task heading regex");
        let mut out = Vec::new();
        let mut custom_kinds = BTreeSet::new();
        for source in sources {
            let raw = fs::read_to_string(&source)
                .map_err(|err| file_io_report(&source, "failed to read mounted plan", err))?;
            let task_text = if layout == TemplateLayout::SingleFile {
                raw.split_once("\n## Tasks\n").map(|(_, tasks)| tasks).unwrap_or(&raw).to_string()
            } else {
                raw
            };
            let mut rendered_text = String::new();
            for line in task_text.lines() {
                let rewritten = if let Some(captures) = heading.captures(line) {
                    let kind = captures.get(2).expect("kind").as_str().to_ascii_lowercase();
                    let qualified_kind = if kind == "task" {
                        "Task".to_string()
                    } else {
                        let qualified = qualifier.qualify(&kind);
                        custom_kinds.insert(qualified.clone());
                        qualified
                    };
                    format!(
                        "{} {} {}:{}",
                        captures.get(1).expect("hashes").as_str(),
                        qualified_kind,
                        qualifier.qualify(captures.get(3).expect("id").as_str()),
                        captures.get(4).expect("title").as_str()
                    )
                } else {
                    qualify_task_metadata_line(line, qualifier, ownership)
                };
                rendered_text.push_str(&rewritten);
                rendered_text.push('\n');
            }
            out.push((source, rendered_text));
        }
        Ok((out, custom_kinds))
    }

    fn collect_markdown_files(root: &Path, out: &mut Vec<PathBuf>) -> MietteResult<()> {
        let mut entries = fs::read_dir(root)
            .map_err(|err| file_io_report(root, "failed to read mounted tasks", err))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| miette!("failed to read mounted task entry: {err}"))?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            if entry.path().is_dir() {
                collect_markdown_files(&entry.path(), out)?;
            } else if entry.path().extension().and_then(|ext| ext.to_str()) == Some("md") {
                out.push(entry.path());
            }
        }
        Ok(())
    }

    fn qualify_task_metadata_line(
        line: &str,
        qualifier: &Qualifier,
        ownership: &SettingsOwnership,
    ) -> String {
        let Some((field, value)) = line.strip_prefix("**").and_then(|line| line.split_once(":**")) else {
            return line.to_string();
        };
        let value = value.trim();
        match field {
            "State" => format!("**State:** {}", qualifier.qualify(value)),
            "Prior" => {
                let refs = value
                    .split(',')
                    .map(|item| {
                        let item = item.trim();
                        let mut parts = item.split_whitespace();
                        let first = parts.next().unwrap_or_default();
                        match parts.next() {
                            Some(id) => format!("{} {}", first, qualifier.qualify(id)),
                            None => qualifier.qualify(first),
                        }
                    })
                    .collect::<Vec<_>>();
                format!("**Prior:** {}", refs.join(", "))
            }
            "Provides" => format!(
                "**Provides:** {}",
                value
                    .split(',')
                    .map(|name| qualifier.qualify(name.trim()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Consumes" => format!(
                "**Consumes:** {}",
                value
                    .split(',')
                    .map(|reference| {
                        reference
                            .trim()
                            .split_once(':')
                            .map(|(task, name)| {
                                format!("{}:{}", qualifier.qualify(task), qualifier.qualify(name))
                            })
                            .unwrap_or_else(|| reference.trim().to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Model" if ownership.models.contains(value) => {
                format!("**Model:** {}", qualifier.qualify(value))
            }
            "Target" => format!("**Target:** {}", qualify_execution_target(value, ownership)),
            _ => line.to_string(),
        }
    }

    fn resolve_leaf_data(
        endpoints: &BTreeMap<String, DataEndpoint>,
        qualifier: &Qualifier,
        machine: &YamlValue,
        tasks: &[(PathBuf, String)],
        dir: &Path,
        output: bool,
    ) -> MietteResult<BTreeMap<String, ResolvedDataEndpoint>> {
        let states = yaml_map(machine, "states")?;
        let all_tasks = tasks.iter().map(|(_, text)| text.as_str()).collect::<Vec<_>>().join("\n");
        endpoints
            .iter()
            .map(|(public, endpoint)| {
                let (state, task, name) = match endpoint.kind {
                    DataKind::StateFile => {
                        let local = endpoint.state.as_deref().ok_or_else(|| {
                            miette!("state-file endpoint '{}' must declare state", public)
                        })?;
                        let state = qualifier.qualify(local);
                        let definition = states
                            .get(YamlValue::String(state.clone()))
                            .and_then(YamlValue::as_mapping)
                            .ok_or_else(|| miette!("endpoint '{}' names missing state '{}'", public, local))?;
                        let section = if output { "outputs" } else { "inputs" };
                        let found = definition
                            .get(YamlValue::String(section.into()))
                            .and_then(YamlValue::as_sequence)
                            .is_some_and(|items| items.iter().any(|item| {
                                item.as_mapping()
                                    .and_then(|item| item.get(YamlValue::String("name".into())))
                                    .and_then(YamlValue::as_str)
                                    == Some(endpoint.name.as_str())
                            }));
                        if !found {
                            return Err(miette!(
                                "data endpoint '{}' in '{}' names missing {} artifact '{}.{}'",
                                public, dir.join("template.yaml").display(), section, local, endpoint.name
                            ));
                        }
                        (Some(state), None, endpoint.name.clone())
                    }
                    DataKind::TaskExport => {
                        let local = endpoint.task.as_deref().ok_or_else(|| {
                            miette!("task-export endpoint '{}' must declare task", public)
                        })?;
                        let task = qualifier.qualify(local);
                        if !all_tasks.contains(&format!(" {task}:")) {
                            return Err(miette!("data endpoint '{}' names missing task '{}'", public, local));
                        }
                        if output
                            && !all_tasks.contains(&format!("**Provides:** {}", qualifier.qualify(&endpoint.name)))
                        {
                            return Err(miette!("task-export endpoint '{}' names missing export '{}:{}'", public, local, endpoint.name));
                        }
                        (None, Some(task), qualifier.qualify(&endpoint.name))
                    }
                };
                Ok((
                    public.clone(),
                    ResolvedDataEndpoint {
                        kind: endpoint.kind,
                        state,
                        task,
                        name,
                        manifest: dir.join("template.yaml"),
                    },
                ))
            })
            .collect()
    }

    fn primary_states(machine: &YamlValue) -> MietteResult<Vec<String>> {
        let root = machine.as_mapping().expect("machine mapping");
        let default = root
            .get(YamlValue::String("node_policy".into()))
            .and_then(YamlValue::as_mapping)
            .and_then(|policy| policy.get(YamlValue::String("default".into())))
            .and_then(YamlValue::as_str);
        if let Some(default) = default {
            if let Some(allowed) = root
                .get(YamlValue::String("profiles".into()))
                .and_then(YamlValue::as_mapping)
                .and_then(|profiles| profiles.get(YamlValue::String(default.into())))
                .and_then(YamlValue::as_mapping)
                .and_then(|profile| profile.get(YamlValue::String("allowed".into())))
                .and_then(YamlValue::as_sequence)
            {
                return Ok(allowed.iter().filter_map(YamlValue::as_str).map(str::to_string).collect());
            }
        }
        Ok(yaml_map(machine, "states")?
            .keys()
            .filter_map(YamlValue::as_str)
            .map(str::to_string)
            .collect())
    }
