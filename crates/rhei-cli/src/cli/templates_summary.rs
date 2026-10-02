    // What `rhei instantiate` prints: the inputs a template declares, and
    // the workspace it just wrote. §FS-rhei-templates.6.1.1
    // §FS-rhei-templates.6.1.3

    /// A default rendered the way the user would supply it on a command line:
    /// one line, flow style, shell-quoted. §FS-rhei-errors.2
    fn compact_default_assignment(input: &TemplateInputDef) -> Option<String> {
        let default = input.schema.default.as_ref()?;
        // `serde_json` emits flow style for both sequences and mappings, which
        // is the syntax `rhei instantiate` parses a value back out of.
        let compact = serde_json::to_string(default).ok()?;
        Some(shell_assignment(&input.name, &compact))
    }

    fn print_template_inputs(manifest: &TemplateManifest, template_ref: &str) {
        println!("Template: {}", manifest.name);
        println!("Version: {}", manifest.version_string());
        println!("Description: {}", manifest.description);

        if manifest.inputs.is_empty() {
            println!("Inputs: none");
            println!();
            println!("Instantiate it with:");
            println!("  {}", shell_command(["rhei", "instantiate", template_ref]));
            return;
        }

        println!("Inputs:");
        for input in &manifest.inputs {
            // A structured default renders as multi-line YAML, which a
            // single-line `(type, default=…)` parenthetical simply tore apart.
            // Scalars stay inline; anything taller gets its own block.
            let rendered_default = input.schema.default.as_ref().map(format_version);
            let block_default =
                rendered_default.as_deref().filter(|rendered| rendered.contains('\n'));
            let requirement = if input.is_required() {
                "required".to_string()
            } else if block_default.is_some() {
                "default below".to_string()
            } else if let Some(default) = rendered_default.as_deref() {
                // §FS-rhei-errors.2: this listing is where users copy values
                // from, and a `[mode]` selector is a glob in zsh unquoted.
                format!("default={}", shell_quote(default))
            } else {
                "optional".to_string()
            };
            println!("  {} ({}, {})", input.name, input.value_type().as_str(), requirement);
            println!("    {}", input.description);
            if let Some(default) = block_default {
                println!("    default:");
                for line in default.lines() {
                    println!("      {line}");
                }
                // §FS-rhei-errors.2: the block above is readable but its
                // scalars are bare YAML, so follow it with a pasteable form.
                if let Some(compact) = compact_default_assignment(input) {
                    println!("    copy: {compact}");
                }
            }
            if let Some(pattern) = input.schema.validate.as_deref() {
                println!("    validate: {}", pattern);
            }
            if let Some(format) = input.schema.format {
                println!("    format: {}", format.as_str());
            }
        }

        // End on the command, not on the inventory.
        // §FS-rhei-errors.1.2 §FS-rhei-templates.6.3
        let required = manifest
            .inputs
            .iter()
            .filter(|input| input.is_required() && input.schema.default.is_none())
            .map(|input| format!("{}={}", input.name, template_input_placeholder(input)))
            .collect::<Vec<_>>();
        println!();
        println!("Instantiate it with:");
        println!(
            "  {}",
            format_template_instantiation_command(template_ref, &[], &[], &[], &[], None, &required)
        );
    }

    fn print_instantiated_workspace_summary(
        materialized: &MaterializedTemplate,
        display_output_dir: &Path,
        state_machine_path: Option<&Path>,
        dry_run: bool,
    ) -> MietteResult<()> {
        let entrypoint = materialized.entrypoint();
        let mut loaded = load_plan(&entrypoint)?;
        let resolved =
            resolve_state_machines_for_loaded_plan(&entrypoint, &loaded, state_machine_path)?;
        // A member loads through its project (§FS-rhei-panta.6), but the
        // summary reports what *this instantiation* created — narrow to the
        // new rhei when siblings are present. §FS-rhei-templates.6.1.3
        let entry_rhei_id = materialized
            .output_dir
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        if loaded.rhei_ids.len() > 1 && loaded.rhei_ids.contains(&entry_rhei_id) {
            loaded
                .rhei
                .tasks
                .retain(|task| task.id.to_string().starts_with(&format!("{entry_rhei_id}.")));
        }
        let machine = resolved
            .per_rhei
            .get(&entry_rhei_id)
            .map(|entry| entry.machine.clone())
            .unwrap_or_else(|| resolved.default.machine.clone());
        let resolved = ResolvedStateMachine { machine, path: resolved.default.path.clone() };
        let tasks = flatten_tasks(&loaded.rhei);

        let display_output_dir = display_path(display_output_dir);
        println!();
        println!("=== Instantiation Summary ===");
        println!("Output: {}", display_output_dir.display());
        println!("Tasks: {}", tasks.len());
        println!("States: {}", format_state_counts(&loaded.rhei));
        println!();

        println!("Files:");
        println!("  {}/", display_output_dir.display());
        print_output_tree(&materialized.output_dir, "  ")?;

        println!();
        println!("Task tree:");
        for task in &loaded.rhei.tasks {
            print_task_tree(task, 1);
        }

        println!();
        println!("Recent task definitions:");
        let last_task_count = tasks.len().min(5);
        for (index, task) in
            tasks.iter().skip(tasks.len().saturating_sub(last_task_count)).enumerate()
        {
            if index > 0 {
                println!();
            }
            println!("--- {} ---", format_task_summary_line(task));
            println!("{}", render_task_definition(task));
        }

        println!();
        println!("Stopped:");
        println!(
            "  {}",
            describe_instantiation_stop(&loaded.rhei, &resolved.machine, &entrypoint, dry_run)
        );

        Ok(())
    }

    fn print_output_tree(root: &Path, prefix: &str) -> MietteResult<()> {
        let mut entries = fs::read_dir(root)
            .map_err(|err| file_io_report(root, "failed to read instantiated output tree", err))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| miette!(
                help = "check that the instantiated output directory is readable.",
                "failed to read dir entry in '{}': {err}", root.display()
            ))?;
        entries.sort_by_key(|entry| entry.file_name());

        let count = entries.len();
        for (idx, entry) in entries.into_iter().enumerate() {
            let path = entry.path();
            let file_type = entry.file_type().map_err(|err| {
                file_io_report(&path, "failed to read instantiated output entry", err)
            })?;
            let is_last = idx + 1 == count;
            let connector = if is_last { "`-- " } else { "|-- " };
            let child_prefix = if is_last { "    " } else { "|   " };
            let name = entry.file_name().to_string_lossy().to_string();

            if file_type.is_dir() {
                println!("{prefix}{connector}{name}/");
                print_output_tree(&path, &format!("{prefix}{child_prefix}"))?;
            } else {
                println!("{prefix}{connector}{name}");
            }
        }

        Ok(())
    }

    fn print_task_tree(task: &rhei_core::ast::Task, depth: usize) {
        println!("{}- {}", "  ".repeat(depth), format_task_summary_line(task));
        for child in &task.children {
            print_task_tree(child, depth + 1);
        }
    }

    fn format_task_summary_line(task: &rhei_core::ast::Task) -> String {
        format!("{} {}: {} [{}]", title_case_kind(&task.kind), task.id, task.title, task.state)
    }

    fn render_task_definition(task: &rhei_core::ast::Task) -> String {
        // Heading level mirrors the on-disk plan, where headings are
        // rhei-local: the qualification segment adds no nesting.
        let heading_level = usize::from(task.profile_level()).saturating_add(2).max(3);
        let mut lines = vec![
            format!(
                "{} {} {}: {}",
                "#".repeat(heading_level),
                title_case_kind(&task.kind),
                task.id,
                task.title
            ),
            format!("**State:** {}", task.state),
        ];

        if !task.prior.is_empty() {
            // Echo each reference as authored: the kind keyword belongs to the
            // referenced node, so inventing `Task` here would misprint plans
            // with custom node kinds. §FS-rhei-plan-language.3.1
            let priors = task
                .prior
                .iter()
                .enumerate()
                .map(|(position, id)| {
                    match task.prior_kinds.get(position).and_then(|k| k.as_deref()) {
                        Some(kind) => format!("{} {id}", title_case_kind(kind)),
                        None => id.to_string(),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("**Prior:** {priors}"));
        }

        if let Some(assignee) = task.assignee.as_deref() {
            lines.push(format!("**Assignee:** {assignee}"));
        }

        let content = task.content.trim();
        if !content.is_empty() {
            lines.push(String::new());
            lines.push(content.to_string());
        }

        lines.join("\n")
    }

    fn describe_instantiation_stop(
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        entrypoint: &Path,
        dry_run: bool,
    ) -> String {
        let tasks = flatten_tasks(rhei);
        if dry_run {
            return "dry run stopped after rendering and validation; no files were written to the requested output path.".to_string();
        }
        if tasks.is_empty() {
            return "instantiation stopped after validation because the rendered workspace has no tasks.".to_string();
        }

        let terminal =
            tasks.iter().filter(|task| is_terminal_state(task.state.as_str(), machine)).count();
        if terminal == tasks.len() {
            return format!(
                "instantiation stopped with the plan already complete: {terminal}/{} tasks are terminal.",
                tasks.len()
            );
        }

        let gating = tasks
            .iter()
            .copied()
            .filter(|task| {
                let state = normalized_state_name(task.state.as_str(), machine);
                machine.states.get(&state).map(|def| def.gating).unwrap_or(false)
            })
            .collect::<Vec<_>>();
        if !gating.is_empty() {
            let labels = gating
                .iter()
                .take(3)
                .map(|task| format_task_summary_line(task))
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if gating.len() > 3 {
                format!(" (+{} more)", gating.len() - 3)
            } else {
                String::new()
            };
            return format!("instantiation stopped at a human gate: {labels}{suffix}.");
        }

        let ready = ready_tasks_from_flat(&tasks, machine);
        if let Some(task) = ready.first() {
            let target = display_path(entrypoint);
            return format!(
                "instantiation stopped before execution; next ready task is {}. Run `rhei run {}` or claim it with `rhei next {}`.",
                format_task_summary_line(task),
                target.display(),
                target.display()
            );
        }

        let blocked = blocked_tasks_from_flat(&tasks, machine);
        if !blocked.is_empty() {
            let labels = blocked
                .iter()
                .take(3)
                .map(|task| format_task_summary_line(task))
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if blocked.len() > 3 {
                format!(" (+{} more)", blocked.len() - 3)
            } else {
                String::new()
            };
            return format!("instantiation stopped with tasks blocked by incomplete prerequisites: {labels}{suffix}.");
        }

        "instantiation stopped after validation; no claimable task was found.".to_string()
    }

    fn ready_tasks_from_flat<'a>(
        tasks: &[&'a rhei_core::ast::Task],
        machine: &rhei_validator::StateMachine,
    ) -> Vec<&'a rhei_core::ast::Task> {
        let state_map: HashMap<&TaskId, String> = tasks
            .iter()
            .map(|task| (&task.id, normalized_state_name(task.state.as_str(), machine)))
            .collect();

        tasks
            .iter()
            .copied()
            .filter(|task| {
                let state = normalized_state_name(task.state.as_str(), machine);
                let gating = machine.states.get(&state).map(|def| def.gating).unwrap_or(false);
                !gating && !is_terminal_state(task.state.as_str(), machine)
            })
            .filter(|task| {
                task.prior.iter().all(|dep| {
                    state_map
                        .get(dep)
                        .map(|state| dependency_is_satisfied(state, machine))
                        .unwrap_or(false)
                })
            })
            .collect()
    }

    fn blocked_tasks_from_flat<'a>(
        tasks: &[&'a rhei_core::ast::Task],
        machine: &rhei_validator::StateMachine,
    ) -> Vec<&'a rhei_core::ast::Task> {
        let state_map: HashMap<&TaskId, String> = tasks
            .iter()
            .map(|task| (&task.id, normalized_state_name(task.state.as_str(), machine)))
            .collect();

        tasks
            .iter()
            .copied()
            .filter(|task| !is_terminal_state(task.state.as_str(), machine))
            .filter(|task| {
                task.prior.iter().any(|dep| {
                    !state_map
                        .get(dep)
                        .map(|state| dependency_is_satisfied(state, machine))
                        .unwrap_or(false)
                })
            })
            .collect()
    }

    /// What laying a project did to each part, said on every run so a skipped
    /// member is never silent. §FS-rhei-library.2.2 §FS-rhei-templates.6.4
    struct ProjectSummary {
        template: String,
        machine: String,
        /// Under `--into`: the root machine the project had, if it had one, and
        /// the tickets the replacement check read.
        into: Option<(Option<String>, CheckedTickets)>,
        members: Vec<String>,
        copied: Vec<String>,
        replaced: Vec<String>,
        settings: Vec<String>,
        warnings: Vec<String>,
    }

    impl ProjectSummary {
        fn new(manifest: &TemplateManifest, machine: &str, root: &ProjectRoot) -> Self {
            Self {
                template: manifest.name.clone(),
                machine: machine.to_owned(),
                into: None,
                members: Vec::new(),
                copied: root.copied.clone(),
                replaced: root.replaced.clone(),
                settings: root.writes.notes.clone(),
                warnings: Vec::new(),
            }
        }

        /// `Laid` where the project had no root machine and `Rebound` where it
        /// had one; a default laid again under its own name was written again
        /// rather than replaced. §FS-rhei-library.2.2
        fn print(&self, project: &Path, dry_run: bool) {
            let at = project_label(project);
            let previous = self.into.as_ref().and_then(|(previous, _)| previous.as_deref());
            match (previous.is_some(), dry_run) {
                (true, false) => println!("Rebound the Panta project at {at}."),
                (false, false) => println!("Laid the Panta project at {at}."),
                (rebind, true) => println!(
                    "Dry run OK: project template '{}' would {} the Panta project at {at}.",
                    self.template,
                    if rebind { "rebind" } else { "lay" }
                ),
            }
            let machine = match previous {
                None => self.machine.clone(),
                Some(previous) if previous == self.machine => {
                    format!("{} written again", self.machine)
                }
                Some(previous) => format!("{} replaced {previous}", self.machine),
            };
            print_summary_lines("default machine:", &[machine]);
            if let Some((_, checked)) = &self.into {
                print_summary_lines("checked:", &[checked_line(checked)]);
            }
            print_summary_lines("members:", &self.members);
            if !self.copied.is_empty() {
                print_summary_lines("copied:", &[self.copied.join(", ")]);
            }
            // A bundle file a rebind replaces is named, never lost silently.
            // §FS-rhei-library.2.2
            if !self.replaced.is_empty() {
                print_summary_lines("replaced:", &[self.replaced.join(", ")]);
            }
            print_summary_lines("settings:", &self.settings);
            for warning in &self.warnings {
                println!("warning: {warning}");
            }
        }
    }

    /// One labelled block of the project summary, its later lines aligned
    /// under the first.
    fn print_summary_lines(label: &str, values: &[String]) {
        for (index, value) in values.iter().enumerate() {
            let label = if index == 0 { label } else { "" };
            println!("  {label:<17}{value}");
        }
    }

    /// `4 tickets in 2 rheis and the basin`. §FS-rhei-library.2.2
    fn checked_line(checked: &CheckedTickets) -> String {
        let scope = match (checked.rheis, checked.basin) {
            (0, false) => return "no rhei runs under the default".to_owned(),
            (0, true) => "the basin".to_owned(),
            (rheis, false) => plural(rheis, "rhei"),
            (rheis, true) => format!("{} and the basin", plural(rheis, "rhei")),
        };
        format!("{} in {scope}", plural(checked.tickets, "ticket"))
    }
