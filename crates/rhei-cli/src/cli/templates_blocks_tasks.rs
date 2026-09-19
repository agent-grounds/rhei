    // Parse once at the authored-source boundary; all semantic edits happen
    // on these shared types in core. §AR-rhei-library.2–3 §FS-rhei-library.8
    impl BlockFrontend {
        fn render_fragment(&mut self, dir: &Path, reference: &str, values: &BTreeMap<String, serde_json::Value>, has_children: bool) -> MietteResult<Fragment> {
            self.render_counter += 1;
            let rendered = self.scratch.path().join(format!("block-{}", self.render_counter));
            let has_plan = dir.join("index.rhei.md").is_file() || dir.join("plan.rhei.md").is_file();
            let layout = if has_plan { detect_template_layout(dir)? } else { TemplateLayout::Workspace };
            materialize_template(dir, reference, layout, &rendered, values, true)?;
            let source = dir.join("states.yaml");
            let mut machine = if rendered.join("states.yaml").is_file() {
                let text = fs::read_to_string(rendered.join("states.yaml")).map_err(|e| file_io_report(&source, "read rendered state fragment", e))?;
                rhei_validator::StateMachine::parse_fragment(&text).map_err(|e| miette!(help = "fix the authored state fragment in states.yaml", "{}: {e}; fix the authored state fragment", source.display()))?
            } else if has_plan && !has_children {
                return Err(miette!(help = "add states.yaml beside the local plan, or mount a child block", "{}: a local mounted plan must supply its states.yaml fragment", source.display()));
            } else {
                rhei_validator::StateMachine { name: reference.into(), version: 1.into(), models: Vec::new(), prompt_templates: Default::default(), states: Default::default(), transitions: Vec::new(), profiles: None, node_policy: None, metrics: Default::default() }
            };
            let mut files = BTreeMap::new();
            read_compiled_files(&rendered, &rendered, &mut files)?;
            files.remove(Path::new("states.yaml"));
            // The compiler compares effective bound prompts, not filenames. §FS-rhei-library.7.1
            for (path, file) in &files {
                if path.parent() == Some(Path::new("prompt_templates")) && path.extension().is_some_and(|extension| extension == "md") {
                    let name = path.file_stem().and_then(|name| name.to_str()).ok_or_else(|| miette!(help = "rename the prompt template file to a valid UTF-8 name", "invalid prompt path {}", path.display()))?;
                    let instructions = String::from_utf8(file.bytes.clone()).map_err(|e| miette!(help = "rewrite the prompt template as UTF-8", "{}: {e}", path.display()))?;
                    machine.prompt_templates.insert(name.into(), rhei_validator::PromptTemplateDef { instructions, source: Some(rendered.join(path)) });
                }
            }
            let settings = files.remove(Path::new(".agent-grounds/rhei/settings.json"))
                .map(|s| serde_json::from_slice(&s.bytes).map_err(|e| miette!(help = "fix the block's settings.json as valid JSON", "{}/settings.json: {e:?}", dir.display())))
                .transpose()?.unwrap_or_else(|| serde_json::json!({}));
            if !has_plan {
                let plan = rhei_core::ast::Rhei { title: reference.into(), states: reference.into(), states_declared: true, structure: Default::default(), metadata: None, content_sections: Vec::new(), tasks: Vec::new() };
                return Ok(Fragment { machine, plan, tasks: Vec::new(), settings, files });
            }
            let (plan, tasks) = match layout {
                TemplateLayout::SingleFile => {
                    let text = files.remove(Path::new("plan.rhei.md")).ok_or_else(|| miette!(help = "add plan.rhei.md to the local block", "missing plan.rhei.md"))?;
                    let text = String::from_utf8(text.bytes).map_err(|e| miette!(help = "rewrite plan.rhei.md as UTF-8", "{}/plan.rhei.md: {e:?}", dir.display()))?;
                    let mut plan = rhei_core::parse(&text).map_err(|e| miette!(help = "fix the plan markdown syntax", "{}/plan.rhei.md: {e:?}", dir.display()))?;
                    let tasks = vec![TaskFile { source_path: PathBuf::from("plan.rhei.md"), path: PathBuf::from("tasks/01-plan.md"), tasks: std::mem::take(&mut plan.tasks) }];
                    (plan, tasks)
                }
                TemplateLayout::Workspace => {
                    let text = files.remove(Path::new("index.rhei.md")).ok_or_else(|| miette!(help = "add index.rhei.md to the block workspace", "missing index.rhei.md"))?;
                    let text = String::from_utf8(text.bytes).map_err(|e| miette!(help = "rewrite index.rhei.md as UTF-8", "{}/index.rhei.md: {e:?}", dir.display()))?;
                    let index = rhei_core::parser::parse_workspace_index(&text).map_err(|e| miette!(help = "fix the workspace index markdown syntax", "{}/index.rhei.md: {e:?}", dir.display()))?;
                    let paths = files.keys().filter(|p| p.starts_with("tasks") && p.extension().is_some_and(|e| e == "md")).cloned().collect::<Vec<_>>();
                    let mut tasks = Vec::new();
                    for path in paths {
                        let text = String::from_utf8(files.remove(&path).unwrap().bytes).map_err(|e| miette!(help = "rewrite the task file as UTF-8", "{}: {e:?}", dir.join(&path).display()))?;
                        let nodes = rhei_core::parser::parse_workspace_tasks_with_structure(&text, &index.structure).map_err(|e| miette!(help = "fix the workspace task markdown syntax", "{}: {e:?}", dir.join(&path).display()))?;
                        tasks.push(TaskFile { source_path: path.clone(), path, tasks: nodes });
                    }
                    (rhei_core::ast::Rhei { title: index.title, states: index.states, states_declared: index.states_declared, structure: index.structure, metadata: index.metadata, content_sections: index.content_sections, tasks: Vec::new() }, tasks)
                }
            };
            Ok(Fragment { machine, plan, tasks, settings, files })
        }
    }

    fn read_compiled_files(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, CompiledFile>) -> MietteResult<()> {
        let entries = fs::read_dir(dir).map_err(|e| file_io_report(dir, "read rendered files", e))?;
        for entry in entries {
            let path = entry.map_err(|e| miette!(help = "check that the block directory is readable", "read block entry: {e:?}"))?.path();
            if path.is_dir() { read_compiled_files(root, &path, files)?; }
            else {
                let data = fs::read(&path).map_err(|e| file_io_report(&path, "read rendered file", e))?;
                let permissions = fs::metadata(&path).map_err(|e| file_io_report(&path, "read rendered permissions", e))?.permissions();
                files.insert(path.strip_prefix(root).unwrap().to_path_buf(), CompiledFile { bytes: data, permissions: Some(permissions) });
            }
        }
        Ok(())
    }
