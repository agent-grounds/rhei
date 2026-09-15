// Editing a task node's metadata block in raw markdown: the `**Assignee:**` a
// claim writes and the `**State:**` a transition rewrites, both inserted where
// the task grammar puts them and both refusing to guess when the file has moved
// under them.
//
// Its own part because these are text edits over one heading's metadata block,
// with no knowledge of what a claim or a transition means.

// §AR-source-file-size.3 §FS-rhei-plan-language.3

/// Insert a `**Assignee:** <value>` metadata line for a specific task.
///
/// Locates the task node header, walks through its metadata block
/// (`**State:**`, optional `**Prior:**`), and inserts the Assignee line at
/// the end of that block, matching the task grammar order. A duplicate
/// insertion is treated as a claim conflict.
// §FS-rhei-plan-language.2: Task metadata grammar order.
fn insert_task_assignee(raw: &str, task_id: &str, assignee: &str) -> MietteResult<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 1);

    let mut in_target_task = false;
    let mut last_metadata_idx: Option<usize> = None;
    let mut already_present = false;
    let mut inserted = false;
    let mut in_code_block = false;

    for line in lines.iter() {
        if let Some(id) = node_heading_id_outside_code(line, &mut in_code_block) {
            if let Some(meta_idx) = last_metadata_idx.take() {
                // Leaving previous target without finding a home for the
                // assignee line — insert immediately after its last metadata
                // line before appending the subsequent task header.
                insert_after(&mut result, meta_idx, &format_assignee(assignee));
                inserted = true;
            }
            in_target_task = id == task_id;
        }

        if !in_code_block && in_target_task && line.starts_with("**Assignee:**") {
            already_present = true;
        }
        if !in_code_block
            && in_target_task
            && (line.starts_with("**State:**")
                || line.starts_with("**Prior:**")
                || line.starts_with("**Provides:**")
                || line.starts_with("**Consumes:**")
                || line.starts_with("**Excludes:**"))
        {
            last_metadata_idx = Some(result.len());
        }

        result.push((*line).to_string());
    }

    if already_present {
        return Err(miette!(
            help = "someone already claimed it. Release it by deleting the **Assignee:** line, \
                    or claim a different task.",
            "Task {} already has an **Assignee:** line",
            task_id
        ));
    }
    if inserted {
        let mut output = result.join("\n");
        if raw.ends_with('\n') {
            output.push('\n');
        }
        return Ok(output);
    }

    let Some(meta_idx) = last_metadata_idx else {
        return Err(miette!(
            help = "every task needs a `**State:** <state>` line under its heading. Add one, \
                    then re-run: rhei validate <plan>",
            "could not find **State:**/**Prior:** metadata line for Task {} in the markdown",
            task_id
        ));
    };
    insert_after(&mut result, meta_idx, &format_assignee(assignee));

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

fn node_heading_id_outside_code<'a>(
    line: &'a str,
    in_code_block: &mut bool,
) -> Option<&'a str> {
    node_heading_outside_code(line, in_code_block).map(|(_, id)| id)
}

fn node_heading_outside_code<'a>(
    line: &'a str,
    in_code_block: &mut bool,
) -> Option<(usize, &'a str)> {
    if line.trim_start().starts_with("```") {
        *in_code_block = !*in_code_block;
        return None;
    }
    if *in_code_block {
        return None;
    }
    node_heading(line)
}

fn node_heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.as_bytes().iter().take_while(|byte| **byte == b'#').count();
    if !(3..=6).contains(&hashes) || !line.as_bytes().get(hashes).is_some_and(|b| *b == b' ') {
        return None;
    }

    let body = &line[hashes + 1..];
    let (prefix, _) = body.split_once(':')?;
    let (_, id) = prefix.rsplit_once(' ')?;
    if id.is_empty() { None } else { Some((hashes, id)) }
}

fn format_assignee(value: &str) -> String {
    format!("**Assignee:** {}", value)
}

fn insert_after(lines: &mut Vec<String>, idx: usize, value: &str) {
    let insert_at = idx + 1;
    if insert_at >= lines.len() {
        lines.push(value.to_string());
    } else {
        lines.insert(insert_at, value.to_string());
    }
}

#[cfg(test)]
mod next_assignee_rewrite_tests {
    use super::*;

    #[test]
    fn insert_assignee_after_state_when_no_prior() {
        let raw = "# Rhei: Test\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\nBody\n";
        let rewritten = insert_task_assignee(raw, "1", "codex").expect("rewrite");
        assert!(rewritten.contains("**State:** pending\n**Assignee:** codex\nBody"));
    }

    #[test]
    fn insert_assignee_after_prior_when_present() {
        let raw =
            "# Rhei: Test\n\n## Tasks\n\n### Task 2: Work\n**State:** pending\n**Prior:** Task 1\nBody\n";
        let rewritten = insert_task_assignee(raw, "2", "codex").expect("rewrite");
        assert!(rewritten.contains("**Prior:** Task 1\n**Assignee:** codex\nBody"));
    }

    #[test]
    fn insert_assignee_supports_child_task_heading() {
        let raw = "# Rhei: Test\n\n## Tasks\n\n### Task 1: Parent\n**State:** pending\n\n#### Task 1.1: Child\n**State:** pending\nBody\n";
        let rewritten = insert_task_assignee(raw, "1.1", "codex").expect("rewrite");
        assert!(rewritten.contains("#### Task 1.1: Child\n**State:** pending\n**Assignee:** codex\nBody"));
        assert!(!rewritten.contains("### Task 1: Parent\n**State:** pending\n**Assignee:**"));
    }

    #[test]
    fn insert_assignee_supports_custom_node_kind() {
        let raw = "# Rhei: Test\n\n## Tasks\n\n### Bug cache-key: Fix cache\n**State:** pending\nBody\n";
        let rewritten = insert_task_assignee(raw, "cache-key", "codex").expect("rewrite");
        assert!(rewritten.contains("### Bug cache-key: Fix cache\n**State:** pending\n**Assignee:** codex\nBody"));
    }

    #[test]
    fn insert_assignee_rejects_existing_assignee() {
        let raw = "# Rhei: Test\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n**Assignee:** alice\nBody\n";
        let err = insert_task_assignee(raw, "1", "codex").expect_err("existing assignee");
        assert!(err.to_string().contains("already has an **Assignee:** line"));
    }

    #[test]
    fn rewrite_state_supports_child_task_heading() {
        let raw = "# Rhei: Test\n\n## Tasks\n\n### Task 1: Parent\n**State:** draft\n\n#### Task 1.1: Child\n**State:** draft\nBody\n";
        let rewritten = rewrite_task_state(raw, "1.1", "pending").expect("rewrite");
        assert!(rewritten.contains("### Task 1: Parent\n**State:** draft"));
        assert!(rewritten.contains("#### Task 1.1: Child\n**State:** pending\nBody"));
    }

    #[test]
    fn insert_assignee_ignores_task_shaped_heading_inside_code_fence() {
        let raw = "# Rhei: Test\n\n## Tasks\n\n### Task 1: Parent\n**State:** pending\n```markdown\n#### Task 1.1: Example\n**State:** draft\n```\n\n#### Task 1.1: Real child\n**State:** pending\nBody\n";
        let rewritten = insert_task_assignee(raw, "1.1", "codex").expect("rewrite");
        assert!(rewritten.contains("#### Task 1.1: Example\n**State:** draft\n```"));
        assert!(rewritten.contains("#### Task 1.1: Real child\n**State:** pending\n**Assignee:** codex\nBody"));
    }

    #[test]
    fn rewrite_state_ignores_task_shaped_heading_inside_code_fence() {
        let raw = "# Rhei: Test\n\n## Tasks\n\n### Task 1: Parent\n**State:** draft\n```markdown\n#### Task 1.1: Example\n**State:** draft\n```\n\n#### Task 1.1: Real child\n**State:** draft\nBody\n";
        let rewritten = rewrite_task_state(raw, "1.1", "pending").expect("rewrite");
        assert!(rewritten.contains("#### Task 1.1: Example\n**State:** draft\n```"));
        assert!(rewritten.contains("#### Task 1.1: Real child\n**State:** pending\nBody"));
    }
}

/// Rewrite the `**State:**` line for a specific task in the raw markdown.
///
/// Locates the task node header and replaces the immediately following
/// `**State:**` line with the new state value.
fn rewrite_task_state(raw: &str, task_id: &str, new_state: &str) -> MietteResult<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result = Vec::with_capacity(lines.len());

    let mut in_target_task = false;
    let mut state_replaced = false;
    let mut in_code_block = false;

    for line in &lines {
        if !state_replaced {
            if let Some(id) = node_heading_id_outside_code(line, &mut in_code_block) {
                in_target_task = id == task_id;
            }
        }

        if !in_code_block && in_target_task && !state_replaced && line.starts_with("**State:**") {
            let formatted = format!("**State:** {}", format_state_metadata_value(new_state));
            result.push(formatted);
            state_replaced = true;
            continue;
        }

        result.push(line.to_string());
    }

    if !state_replaced {
        return Err(miette!(
            help = "add a `**State:** <state>` line under the task heading, then re-run: \
                    rhei validate <plan>",
            "could not find **State:** line for Task {} in the markdown",
            task_id
        ));
    }

    // Preserve trailing newline if original had one.
    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}
