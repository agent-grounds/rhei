/// Execute the `render` subcommand for the selected output format.
#[allow(clippy::too_many_arguments)]
fn render_command(
    input: &Path,
    rhei_scope: &[String],
    state_machine_path: Option<&Path>,
    format: RenderFormat,
    pretty: bool,
    no_color: bool,
    no_metadata: bool,
    no_content: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let rhei = narrow_rhei_to_scope(&loaded.rhei, &scope);
    // The completion summary needs to know which tickets are done — each
    // judged under its owning rhei's machine. A plan whose machines will not
    // resolve still renders — it just renders without the summary.

    // §DA-per-rhei-state-machines
    let terminal_ids = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)
        .map(|resolved| terminal_ticket_ids(&rhei, &resolved.validator_set()))
        .unwrap_or_default();
    let rendered = render_rhei(
        input,
        &rhei,
        terminal_ids,
        loaded.is_panta_project(),
        rhei_machine_attribution(&loaded, &scope),
        format,
        pretty,
        no_color,
        no_metadata,
        no_content,
    )?;
    println!("{rendered}");
    Ok(())
}

/// Each in-scope rhei paired with the machine it actually runs, for the JSON
/// document's `rheis` array. Empty for a plan that is not a merged project.
/// §FS-rhei-render.3.1 §DA-per-rhei-state-machines
fn rhei_machine_attribution(
    loaded: &LoadedPlan,
    scope: &RheiScope,
) -> Vec<rhei_output::RheiMachine> {
    if !loaded.is_panta_project() {
        return Vec::new();
    }
    loaded
        .rhei_ids
        .iter()
        .filter(|id| scope.as_ref().is_none_or(|ids| ids.contains(id.as_str())))
        .map(|id| match loaded.rhei_machines.get(id) {
            Some(declared) => rhei_output::RheiMachine {
                id: id.clone(),
                states: declared.clone(),
                declared: true,
            },
            None => rhei_output::RheiMachine {
                id: id.clone(),
                states: loaded.rhei.states.clone(),
                declared: false,
            },
        })
        .collect()
}

/// Every ticket currently in a terminal state, judged per owning machine, for
/// the progress summary. §DA-per-rhei-state-machines
fn terminal_ticket_ids(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> BTreeSet<String> {
    fn walk(
        tasks: &[rhei_core::ast::Task],
        machines: &rhei_validator::MachineSet,
        out: &mut BTreeSet<String>,
    ) {
        for task in tasks {
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            if machine.states.get(&state).map(|def| def.terminal).unwrap_or(false) {
                out.insert(task.id.to_string());
            }
            walk(&task.children, machines, out);
        }
    }
    let mut out = BTreeSet::new();
    walk(&rhei.tasks, machines, &mut out);
    out
}

/// Drop tickets and content sections outside `scope`, keeping the plan's own
/// title and manifest sections. Priors keep their project-qualified ids: they
/// were resolved against the whole project. §FS-rhei-panta.6
fn narrow_rhei_to_scope(
    rhei: &rhei_core::ast::Rhei,
    scope: &RheiScope,
) -> rhei_core::ast::Rhei {
    if scope.is_none() {
        return rhei.clone();
    }
    let mut narrowed = rhei.clone();
    narrowed.tasks.retain(|task| task_in_rhei_scope(scope, &task.id.to_string()));
    narrowed.content_sections.retain(|section| match section.rhei.as_deref() {
        Some(id) => task_in_rhei_scope(scope, id),
        None => true,
    });
    narrowed
}

/// Render a parsed rhei into the requested output representation.
///
/// Fallible only on the JSON format, and only because frontmatter is YAML: a
/// value JSON has no image for is named against the plan rather than dropped
/// from the document. §FS-rhei-render.3.1.1
#[allow(clippy::too_many_arguments)]
fn render_rhei(
    input: &Path,
    rhei: &rhei_core::ast::Rhei,
    terminal_ids: BTreeSet<String>,
    is_project: bool,
    rhei_machines: Vec<rhei_output::RheiMachine>,
    format: RenderFormat,
    pretty: bool,
    no_color: bool,
    no_metadata: bool,
    no_content: bool,
) -> MietteResult<String> {
    match format {
        RenderFormat::Json => {
            let value = rhei_output::to_json_value_with_rheis(rhei, rhei_machines)
                .map_err(|found| unrepresentable_frontmatter_error(input, &found))?;
            let rendered = if pretty {
                serde_json::to_string_pretty(&value)
            } else {
                serde_json::to_string(&value)
            };
            rendered.map_err(|err| {
                miette!(help = internal_error_help(), "failed to serialize JSON output: {err}")
            })
        }
        RenderFormat::Github => Ok(rhei_output::GithubIssuesOutput {
            include_content: !no_content,
            include_metadata: !no_metadata,
        }
        .to_markdown(rhei)),
        RenderFormat::Progress => {
            let color = should_use_color(no_color);
            Ok(rhei_output::ProgressReportOutput {
                color,
                show_dependencies: true,
                terminal_ids,
                is_project,
            }
            .to_string(rhei))
        }
    }
}

/// Decide whether ANSI color should be emitted for progress output.
///
/// Precedence: explicit `--no-color` always wins. Otherwise honour the
/// `NO_COLOR` environment variable (any non-empty value disables color) and
/// fall back to stdout TTY detection.
fn should_use_color(no_color_flag: bool) -> bool {
    use std::io::IsTerminal;
    if no_color_flag {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    std::io::stdout().is_terminal()
}

/// Print versions for the CLI and the crates surfaced by this command.
fn print_versions() {
    println!("rhei-cli {}", env!("CARGO_PKG_VERSION"));
    println!("rhei-core {}", rhei_core::version());
    println!("rhei-validator {}", rhei_validator::version());
    println!("rhei-output {}", rhei_output::version());
}

/// Handler for the `install-skills` subcommand.
///
/// Resolves the agent list (expanding `All`), iterates over each agent,
/// and calls the appropriate install/uninstall handler.
fn install_skills_command(
    agent: Agent,
    local: bool,
    link: bool,
    uninstall: bool,
    dry_run: bool,
    skills: &[String],
) -> MietteResult<()> {
    let agents = expand_agent_list(agent);
    let mut installed_count = 0u32;

    let project_root = if local { Some(find_project_root()?) } else { None };

    // Resolve all skill sources up front. `resolved` owns any extraction of the
    // embedded skills, so it has to outlive every install below.
    let resolved =
        if uninstall { None } else { Some(resolve_skill_sources(skills, link)?) };
    let skill_sources: &[(String, PathBuf)] =
        resolved.as_ref().map(|r| r.sources.as_slice()).unwrap_or(&[]);

    for ag in &agents {
        let label = agent_label(ag);
        let mode_suffix = if local { " (local)" } else { "" };
        println!("\n{}{}:", label, mode_suffix);

        let result = if uninstall {
            uninstall_agent(ag, local, dry_run, skills, project_root.as_deref())
        } else {
            install_agent(ag, local, link, dry_run, skill_sources, project_root.as_deref())
        };

        match result {
            Ok(()) => installed_count += 1,
            // The diagnostic, not just its message: `{e}` renders the
            // message alone and drops the `help =` that says what to do
            // instead — which is the whole point of `--link`'s platform
            // refusal, since it names plain copying as the way through.
            // `{e:?}` is what the rest of the CLI prints a report with.

            // §FS-rhei-errors.1.2 §FS-rhei-install-skills.4.4
            Err(e) => eprintln!("{e:?}"),
        }
    }

    let action = if uninstall { "Uninstalled" } else { "Installed" };
    let scope = if local { " locally" } else { "" };
    println!(
        "\n{} rhei skills{} for {} agent{}.",
        action,
        scope,
        installed_count,
        if installed_count == 1 { "" } else { "s" }
    );
    // The default fans out across every supported agent, which writes config
    // for tools the user may not have. Name the narrower form once. §FS-rhei-install-skills.1
    if agent == Agent::All && installed_count > 1 && !uninstall {
        println!(
            "That was every supported agent (the `--agent` default). Use `--agent <name>` \
             to write only the one you use, or `--uninstall` to remove these again."
        );
    }

    Ok(())
}

/// Expand the `All` agent variant into the full list of concrete agents.
fn expand_agent_list(agent: Agent) -> Vec<Agent> {
    if agent == Agent::All {
        vec![
            Agent::ClaudeCode,
            Agent::Cursor,
            Agent::Windsurf,
            Agent::Copilot,
            Agent::Kilocode,
            Agent::Pi,
            Agent::Codex,
            Agent::Antigravity,
        ]
    } else {
        vec![agent]
    }
}

/// Human-readable label for an agent.
fn agent_label(agent: &Agent) -> &'static str {
    match agent {
        Agent::ClaudeCode => "claude-code",
        Agent::Cursor => "cursor",
        Agent::Windsurf => "windsurf",
        Agent::Copilot => "copilot",
        Agent::Kilocode => "kilocode",
        Agent::Pi => "pi",
        Agent::Codex => "codex",
        Agent::Antigravity => "antigravity",
        Agent::All => "all",
    }
}

/// Home directory helper.
fn home_dir() -> MietteResult<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| miette!(
            help = "rhei writes user-level files under $HOME. Set HOME, or install into the project with --local.",
            "HOME environment variable not set"
        ))
}

/// Install skills for a single agent.
fn install_agent(
    agent: &Agent,
    local: bool,
    link: bool,
    dry_run: bool,
    skill_sources: &[(String, PathBuf)],
    project_root: Option<&Path>,
) -> MietteResult<()> {
    match agent {
        Agent::ClaudeCode => install_claude_code(skill_sources, local, link, dry_run, project_root),
        Agent::Cursor => install_cursor(skill_sources, local, link, dry_run, project_root),
        Agent::Windsurf => install_windsurf(skill_sources, local, dry_run, project_root),
        Agent::Copilot => install_copilot(skill_sources, local, dry_run, project_root),
        Agent::Kilocode => {
            install_rules_dir_agent(".kilocode", skill_sources, local, link, dry_run, project_root)
        }
        Agent::Pi => {
            install_rules_dir_agent(".pi", skill_sources, local, link, dry_run, project_root)
        }
        Agent::Codex => install_codex(skill_sources, local, link, dry_run, project_root),
        Agent::Antigravity => install_rules_dir_agent(
            ".antigravity",
            skill_sources,
            local,
            link,
            dry_run,
            project_root,
        ),
        Agent::All => Ok(()), // handled by expand_agent_list
    }
}

/// Install skills for Claude Code.
fn install_claude_code(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        require_project_root(project_root)?.join(".claude")
    } else {
        home_dir()?.join(".claude")
    };

    let skills_dir = base.join("skills");

    // Install each skill directory.
    for (name, source) in skill_sources {
        let dest = skills_dir.join(name);
        if link {
            let src =
                if local { relative_path(dest.parent().unwrap(), source) } else { source.clone() };
            link_skill(&src, &dest, dry_run)?;
        } else {
            copy_skill(source, &dest, dry_run)?;
        }
    }

    // Generate and inject registration block into CLAUDE.md.
    let claude_md = base.join("CLAUDE.md");
    let mut block = String::from("# rhei\n");
    for (name, _) in skill_sources {
        let skill_path = if local {
            format!(".claude/skills/{name}/SKILL.md")
        } else {
            format!("~/.claude/skills/{name}/SKILL.md")
        };
        let description = skill_description(name);
        let trigger = format!("/{name}");
        block.push_str(&format!(
            "- **{name}** (`{skill_path}`) — {description}. Trigger: `{trigger}`\n"
        ));
    }
    let trigger_list: Vec<String> =
        skill_sources.iter().map(|(name, _)| format!("`/{name}`")).collect();
    block.push_str(&format!(
        "When the user types {}, invoke the Skill tool with the corresponding skill name before doing anything else.\n",
        trigger_list.join(", ")
    ));

    // Use heading-based injection for Claude Code (not HTML markers).
    inject_claude_md_section(&claude_md, &block, dry_run)?;

    println!("  ✓ {} — registered {} skills", claude_md.display(), skill_sources.len());

    Ok(())
}

/// Inject or replace a `# rhei` section in a CLAUDE.md file.
fn inject_claude_md_section(file: &Path, content: &str, dry_run: bool) -> MietteResult<()> {
    let existing = if file.exists() {
        fs::read_to_string(file).map_err(|e| file_io_report(file, "failed to read", e))?
    } else {
        String::new()
    };

    // Replace an existing `# rhei` section in place, taking only the block
    // itself: the rest of the file is the user's. §FS-rhei-install-skills.4.5
    let lines: Vec<&str> = existing.lines().collect();
    let mut new_lines: Vec<String> = Vec::new();
    let mut replaced = false;
    let mut index = 0;

    while index < lines.len() {
        if !replaced && (lines[index] == "# rhei" || lines[index] == "## rhei") {
            new_lines.extend(content.lines().map(ToOwned::to_owned));
            replaced = true;
            index = claude_md_block_end(&lines, index);
            continue;
        }
        new_lines.push(lines[index].to_string());
        index += 1;
    }

    if !replaced {
        // Append the section.
        if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true) {
            new_lines.push(String::new());
        }
        for cl in content.lines() {
            new_lines.push(cl.to_string());
        }
    }

    let mut final_content = new_lines.join("\n");
    if !final_content.ends_with('\n') {
        final_content.push('\n');
    }

    if dry_run {
        println!("  [dry-run] would update {}", file.display());
        return Ok(());
    }

    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| file_io_report(parent, "failed to create directory", e))?;
    }
    fs::write(file, &final_content).map_err(|e| file_io_report(file, "failed to write", e))?;

    Ok(())
}
