// `## Position`: where this invocation sits in the project, top down — the
// chain from the Panta to this ticket, the siblings beside it, the parent whose
// decomposition it belongs to, and, for ordinary work, the standing notes of
// its rhei and project.
//
// Its own part because orientation is composed from the plan tree alone, while
// the history beside it is composed from the runtime tree.

// §AR-source-file-size.3 §FS-rhei-memory.3.1 §FS-rhei-memory.4.2

/// One content section, rendered back to the document shape it was authored in.
///
/// A Panta merge retitles a rhei's sections `Rhei <id> / <title>` so a flat
/// list still reads; pasting them back under the rhei they came from restores
/// the authored heading.
// §AR-rhei-panta.3
fn content_section_block(section: &rhei_core::ast::ContentSection, rhei_id: &str) -> String {
    let prefix = format!("Rhei {rhei_id} / ");
    let title = section.title.strip_prefix(&prefix).unwrap_or(section.title.as_str());
    format!("## {title}\n\n{}", section.content.trim())
}

/// The content sections one scope contributed, in authored order.
///
/// A merged project marks each section with its owning rhei and leaves the
/// manifest's own unmarked; a bare rhei's implicit Panta marks nothing, so its
/// unmarked sections are the rhei's own and it has no project scope at all.
// §FS-rhei-memory.3.1 §FS-rhei-memory.4.2
fn scoped_content_sections(memory: &PromptMemory, rhei_id: Option<&str>) -> String {
    let owner = rhei_id.filter(|_| memory.explicit_panta);
    memory
        .content_sections
        .iter()
        .filter(|section| section.rhei.as_deref() == owner)
        .filter(|section| !section.content.trim().is_empty())
        .map(|section| content_section_block(section, rhei_id.unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// One pasted context block: fenced, capped, and told where the rest is.
// §FS-rhei-memory.4.2 §FS-rhei-memory.4.5
fn render_context_block(heading: &str, body: &str, source: &str) -> String {
    if body.trim().is_empty() {
        return String::new();
    }
    let (kept, truncated) = head_lines(body, memory_caps::CONTEXT_LINES);
    let overflow =
        if truncated { format!("\n\u{2026} truncated; read {source}\n") } else { String::new() };
    format!("\n### {heading}\n\n{}\n{overflow}", fenced_markdown(&kept))
}

/// Whether `candidate` waits on `subject`: a declared prior, or an export of
/// `subject` it consumes. §FS-rhei-memory.4.2
fn task_waits_on(candidate: &rhei_core::ast::Task, subject: &rhei_core::ast::Task) -> bool {
    if candidate.prior.iter().any(|prior| prior == &subject.id) {
        return true;
    }
    candidate.consumes.iter().any(|consumed| {
        consumed.task == subject.id && subject.provides.contains(&consumed.name)
    })
}

/// The parent's other children, in plan order, each marked when it waits.
// §FS-rhei-memory.4.2
fn render_siblings(
    render_context: &RuntimeTemplateContext<'_>,
    parent: &rhei_core::ast::Task,
) -> String {
    let siblings: Vec<&rhei_core::ast::Task> =
        parent.children.iter().filter(|child| child.id != render_context.task.id).collect();
    if siblings.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n### Siblings\n\n");
    for sibling in siblings.iter().take(memory_caps::SIBLINGS) {
        let marker = if task_waits_on(sibling, render_context.task) {
            " \u{2014} waits on this task"
        } else {
            ""
        };
        out.push_str(&format!(
            "- {}: {} [{}]{marker}\n",
            memory_node_label(sibling),
            sibling.title,
            memory_state_name(sibling, render_context.machine)
        ));
    }
    if siblings.len() > memory_caps::SIBLINGS {
        out.push_str(&format!(
            "\u{2026} {} more \u{2014} rhei list --parent {}\n",
            siblings.len() - memory_caps::SIBLINGS,
            parent.id
        ));
    }
    out
}

/// The nearest ancestor's body: where the decomposition was decided and where
/// the acceptance for the whole subtree is written. Higher ancestors contribute
/// their chain line and nothing more.
// §FS-rhei-memory.3.1 §FS-rhei-memory.4.2
fn render_parent_body(
    render_context: &RuntimeTemplateContext<'_>,
    memory: &PromptMemory,
    parent: &rhei_core::ast::Task,
) -> String {
    let body = parent.content.trim();
    if body.is_empty() {
        return String::new();
    }
    let (kept, truncated) = head_lines(body, memory_caps::PARENT_BODY_LINES);
    let overflow = if truncated {
        let source = memory
            .task_sources
            .get(&parent.id.to_string())
            .map(|path| memory_path(render_context, path))
            .unwrap_or_else(|| memory_path(render_context, render_context.plan_path));
        format!("\n\u{2026} truncated; read {source}\n")
    } else {
        String::new()
    };
    format!(
        "\n### Parent: {}: {}\n\n{}\n{overflow}",
        memory_node_label(parent),
        parent.title,
        fenced_markdown(&kept)
    )
}

/// The chain from the Panta down to this invocation, one ancestor per hop.
// §FS-rhei-memory.3.1 §FS-rhei-memory.4.2
fn render_position_chain(
    render_context: &RuntimeTemplateContext<'_>,
    memory: &PromptMemory,
    ancestors: &[&rhei_core::ast::Task],
) -> String {
    let mut chain = format!("Panta: {}", memory.panta_title);
    if let Some(rhei_id) = owning_rhei_id(render_context) {
        let title = memory.rhei_titles.get(&rhei_id).cloned().unwrap_or_default();
        chain.push_str(&format!(" \u{203a} rhei `{rhei_id}`: {title}"));
    }
    for ancestor in ancestors.iter().rev() {
        chain.push_str(&format!(
            " \u{203a} {}: {} [{}]",
            memory_node_label(ancestor),
            ancestor.title,
            memory_state_name(ancestor, render_context.machine)
        ));
    }
    let visit = render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    );
    format!(
        "{chain}\n\u{203a} **{}: {} [{}]** \u{2190} this invocation (visit {visit})\n",
        memory_node_label(render_context.task),
        render_context.task.title,
        render_context.state_name
    )
}

/// `## Position` — the orientation an instruction is meant to be read with, so
/// it comes before the instructions rather than after them.
// §FS-rhei-memory.3.1 §FS-rhei-memory.4.2
fn render_position(render_context: &RuntimeTemplateContext<'_>) -> String {
    let Some(memory) = render_context.memory else { return String::new() };
    let ancestors = render_context
        .plan_tasks
        .map(|tasks| ancestor_chain(tasks, &render_context.task.id))
        .unwrap_or_default();
    let mut out = String::from("\n## Position\n\n");
    out.push_str(&render_position_chain(render_context, memory, &ancestors));
    if let Some(parent) = ancestors.first() {
        out.push_str(&render_siblings(render_context, parent));
        let parent_source = memory
            .task_sources
            .get(&parent.id.to_string())
            .map(PathBuf::as_path)
            .unwrap_or(render_context.plan_path);
        if prompt_source_allowed(render_context, parent_source) {
            out.push_str(&render_parent_body(render_context, memory, parent));
        }
    }
    // Supervisors keep the navigation map rendered later in the prompt, so
    // repository-scale standing context remains reachable without being pasted.
    // §FS-rhei-memory.4.2
    if task_is_supervising(render_context.task, render_context.machine) {
        return out;
    }
    let rhei_id = owning_rhei_id(render_context);
    let rhei_source = rhei_id
        .as_deref()
        .and_then(|id| memory.rhei_plans.get(id))
        .map(PathBuf::as_path)
        .unwrap_or(render_context.plan_path);
    if prompt_source_allowed(render_context, rhei_source) {
        out.push_str(&render_context_block(
            "Rhei Context",
            &scoped_content_sections(memory, rhei_id.as_deref()),
            &memory_path(render_context, rhei_source),
        ));
    }
    if let Some(manifest) = memory.panta_manifest.as_deref() {
        if prompt_source_allowed(render_context, manifest) {
            out.push_str(&render_context_block(
                "Project Context",
                &scoped_content_sections(memory, None),
                &memory_path(render_context, manifest),
            ));
        }
    }
    out
}
