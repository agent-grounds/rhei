// What `rhei next` prints once a ticket is picked: the human transcript a
// worker reads and the JSON a scripted one parses.
//
// Its own part because this renders one claimed ticket, while the file next
// door renders a whole plan and installs agent skills — different inputs,
// different audiences, no shared helper.

// §AR-source-file-size.3 §FS-rhei-next.4

struct NextOutput<'a> {
    as_json: bool,
    peek: bool,
    /// The assignee this invocation wrote, when it claimed the ticket. `None`
    /// under `--peek` and when the ticket was already claimed.
    claimed_as: Option<&'a str>,
    task: &'a rhei_core::ast::Task,
    /// The authored `**State:**` values, suffix and all — what JSON reports.
    from_state: &'a str,
    to_state: &'a str,
    /// The machine's own names for the same two states: the text surface prints
    /// these, so one screen spells a state one way and every name on it is a
    /// name `rhei transition --from` accepts. The visit is in `## Position`.
    // §FS-rhei-next.4.1 §FS-rhei-memory.3.1
    from_state_name: &'a str,
    to_state_name: &'a str,
    /// The machine this ticket runs under, so a child of the list below is
    /// spelled the way the invocation's own state is. §FS-rhei-next.4.1
    machine: &'a rhei_validator::StateMachine,
    personality: Option<&'a str>,
    instructions: &'a str,
    /// The supervisor's `## Checkpoints` section, or empty when this ticket is
    /// not in a supervising state or is owed no checkpoints.
    // §FS-rhei-supervision.3.4 §FS-rhei-supervision.5.1
    checkpoints: &'a str,
    /// The `## Supervisor Brief` section a supervising ancestor wrote for this
    /// ticket, or empty when there is none.
    // §FS-rhei-supervision.5.2
    supervisor_brief: &'a str,
    /// The `## Supervising This Subtree` notes a supervising ticket's manual
    /// worker needs, or empty when this ticket does not supervise.
    // §FS-rhei-supervision.3.4
    supervising: &'a str,
    /// The four mid-term memory sections, each empty when the run prompt would
    /// not carry it either. §FS-rhei-memory.5
    position: &'a str,
    plan_history: &'a str,
    previous_visits: &'a str,
    navigation: &'a str,
    /// Resolved source boundary; manual work is always composition-only.
    // §FS-rhei-next.3.1
    exclusions: &'a str,
    agent_id: Option<&'a str>,
    model_id: Option<&'a str>,
}

/// Print the `next` command output in either human-readable or JSON format.
fn print_next_output(output: NextOutput<'_>) {
    fn child_json(task: &rhei_core::ast::Task) -> serde_json::Value {
        let children: Vec<serde_json::Value> = task.children.iter().map(child_json).collect();
        serde_json::json!({
            "id": task.id.to_string(),
            "kind": task.kind,
            "title": task.title,
            "state": task.state,
            "content": task.content.trim(),
            "children": children,
        })
    }

    if output.as_json {
        let children: Vec<serde_json::Value> =
            output.task.children.iter().map(child_json).collect();

        let mut obj = serde_json::json!({
            "task_id": output.task.id.to_string(),
            "kind": output.task.kind,
            "title": output.task.title,
            "from_state": output.from_state,
            "state": output.to_state,
            "personality": output.personality,
            "instructions": output.instructions,
            "content": output.task.content.trim(),
            "children": children,
        });
        // Present exactly when this invocation took the claim, so a scripted
        // worker can tell a claim from a peek without re-reading the plan.
        // §FS-rhei-next.4
        if let Some(assignee) = output.claimed_as {
            obj["claimed_as"] = serde_json::json!(assignee);
        }
        if let Some(agent) = output.agent_id {
            obj["agent"] = serde_json::json!(agent);
        }
        if let Some(model) = output.model_id {
            obj["model"] = serde_json::json!(model);
        }
        // Present only where the run prompt would carry the section, so a plan
        // without supervision keeps its document. §FS-rhei-supervision.3.4
        if !output.checkpoints.is_empty() {
            obj["checkpoints"] = serde_json::json!(output.checkpoints.trim());
        }
        if !output.supervisor_brief.is_empty() {
            obj["supervisor_brief"] = serde_json::json!(output.supervisor_brief.trim());
        }
        if !output.supervising.is_empty() {
            obj["supervising"] = serde_json::json!(output.supervising.trim());
        }
        // §FS-rhei-memory.5: one string field per section, named after it and
        // omitted when the section is empty, exactly as `checkpoints` is.
        for (field, section) in [
            ("position", output.position),
            ("plan_history", output.plan_history),
            ("previous_visits", output.previous_visits),
            ("navigation", output.navigation),
            ("exclusions", output.exclusions),
        ] {
            if !section.is_empty() {
                obj[field] = serde_json::json!(section.trim());
            }
        }
        println!("{}", serde_json::to_string_pretty(&obj).expect("JSON serialization"));
    } else {
        let transitioned = output.from_state_name != output.to_state_name;
        if output.peek {
            println!(
                "Task {} — current state: '{}' (read-only peek; not advanced)",
                output.task.id, output.to_state_name
            );
        } else if transitioned {
            println!(
                "Task {} claimed: '{}' -> '{}'",
                output.task.id, output.from_state_name, output.to_state_name
            );
        } else if let Some(assignee) = output.claimed_as {
            // A claim that does not move the ticket is still a claim, and
            // it still wrote the `**Assignee:**` that stops a second worker.
            // §FS-rhei-next.4: claim mode reports the claim it took.
            println!(
                "Task {} claimed by {} (stays in '{}')",
                output.task.id, assignee, output.to_state_name
            );
        } else {
            println!("Task {} (already in '{}')", output.task.id, output.to_state_name);
        }
        if output.agent_id.is_some() || output.model_id.is_some() {
            let agent_str = output.agent_id.unwrap_or("none");
            let model_str = output.model_id.unwrap_or("default");
            println!("Agent: {}  |  Model: {}", agent_str, model_str);
        }
        if let Some(personality) = output.personality {
            println!();
            println!("Personality: {}", personality);
        }
        println!();
        println!("## Task {}: {}", output.task.id, output.task.title);
        if !output.task.content.trim().is_empty() {
            println!();
            println!("{}", output.task.content.trim());
        }
        if !output.task.children.is_empty() {
            println!();
            for child in &output.task.children {
                // The machine's name for the state, not the authored one: a
                // `-<n>` beside a normalized own state reads as two machines.
                // §FS-rhei-next.4.1 §FS-rhei-memory.4.5
                println!(
                    "  - {} {}: {} [{}]",
                    title_case_kind(&child.kind),
                    child.id,
                    child.title,
                    memory_state_name(child, output.machine)
                );
                if !child.content.trim().is_empty() {
                    for line in child.content.trim().lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
        if !output.instructions.is_empty() {
            println!();
            println!("--- Instructions ({}) ---", output.to_state_name);
            println!("{}", output.instructions);
        }
        // The run prompt's own sections, in its order: `supervising` says what
        // `## Rhei Commands` does, so it precedes that section's map here too.
        // §FS-rhei-supervision.3.4 §FS-rhei-memory.5
        for section in [
            output.exclusions,
            output.position,
            output.checkpoints,
            output.supervisor_brief,
            output.plan_history,
            output.previous_visits,
            output.supervising,
            output.navigation,
        ] {
            if !section.is_empty() {
                print!("{section}");
            }
        }
    }
}
