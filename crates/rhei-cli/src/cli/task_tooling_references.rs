/// Every id a task names in `**MCP servers:**` and `**Skills:**` must resolve
/// against the merged registries, checked per task and whatever states it
/// passes through: an id a withholding state will drop is still checked, and
/// withholding itself is never reported. §FS-rhei-task-tooling.6
fn validate_task_tooling_settings_references(
    rhei: &rhei_core::ast::Rhei,
    settings: &RheiSettings,
) -> Vec<String> {
    fn visit(task: &rhei_core::ast::Task, settings: &RheiSettings, errors: &mut Vec<String>) {
        let ids = |entries: &[TaskToolingEntry]| {
            entries.iter().map(|entry| entry.id.clone()).collect::<Vec<_>>()
        };
        let mcp_servers: Vec<StateMcpEntry> =
            ids(&task.tooling.mcp_servers).into_iter().map(StateMcpEntry::Id).collect();
        let label = format!("Task {} {MCP_SERVERS_FIELD}", task.id);
        validate_mcp_entries_known(&label, Some(&mcp_servers), &settings.mcp_servers, errors);
        let skills: Vec<StateSkillEntry> =
            ids(&task.tooling.skills).into_iter().map(StateSkillEntry::Id).collect();
        let label = format!("Task {} {SKILLS_FIELD}", task.id);
        validate_skill_entries_known(&label, Some(&skills), &settings.skills, errors);
        for child in &task.children {
            visit(child, settings, errors);
        }
    }
    let mut errors = Vec::new();
    for task in &rhei.tasks {
        visit(task, settings, &mut errors);
    }
    errors
}
