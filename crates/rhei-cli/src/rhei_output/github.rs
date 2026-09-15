use rhei_core::ast::Task;

use crate::rhei_output::common::{fmt_prior_list, rhei_groups, title_case_kind, RheiGroup};

pub struct GithubIssuesOutput {
    pub include_content: bool,
    pub include_metadata: bool,
}

impl GithubIssuesOutput {
    /// Render the provided Rhei into a single GitHub-friendly Markdown document.
    pub fn to_markdown(&self, rhei: &rhei_core::ast::Rhei) -> String {
        let mut out = String::new();

        out.push_str("# Rhei: ");
        out.push_str(&rhei.title);
        out.push_str("\n\n");

        for section in &rhei.content_sections {
            // Project-contributed sections are printed inside their rhei's
            // block; only the manifest's own sections belong up here.
            if section.rhei.is_some() {
                continue;
            }
            out.push_str("## ");
            out.push_str(&section.title);
            out.push('\n');
            if !section.content.is_empty() {
                out.push_str(section.content.trim_end());
                out.push('\n');
            }
            out.push('\n');
        }

        let groups = rhei_groups(rhei);
        match groups.as_slice() {
            // A plan that is not a merged project keeps the flat shape: one
            // `## Tasks` chapter, mirroring how it was authored.
            [] => {
                out.push_str("## Tasks\n\n");
                for task in &rhei.tasks {
                    self.render_node(task, 3, &mut out);
                }
            }
            groups => {
                for group in groups {
                    self.render_group(rhei, group, &mut out);
                }
            }
        }

        out.trim_end().to_string()
    }

    /// Render one rhei of a merged project: its heading, its own content
    /// sections, then its tickets — so the document reads workstream by
    /// workstream instead of as one flat list under shared headings.
    fn render_group(&self, rhei: &rhei_core::ast::Rhei, group: &RheiGroup, out: &mut String) {
        out.push_str("## ");
        out.push_str(&group.heading());
        out.push_str("\n\n");

        for section in &rhei.content_sections {
            if section.rhei.as_deref() != Some(group.id.as_str()) || section.content.is_empty() {
                continue;
            }
            out.push_str("### ");
            out.push_str(group.section_title(&section.title));
            out.push('\n');
            out.push_str(section.content.trim_end());
            out.push_str("\n\n");
        }

        let mut empty = true;
        for task in rhei.tasks.iter().filter(|task| group.owns(task)) {
            self.render_node(task, 3, out);
            empty = false;
        }
        if empty {
            // A rhei with no tickets is a real, valid state (a freshly created
            // one). Saying so beats a heading with nothing under it.
            out.push_str("_No tickets yet._\n\n");
        }
    }

    fn render_node(&self, task: &Task, level: u8, out: &mut String) {
        let hashes = "#".repeat(level as usize);
        out.push_str(&hashes);
        out.push(' ');
        out.push_str(&title_case_kind(&task.kind));
        out.push(' ');
        out.push_str(&task.id.to_string());
        out.push_str(": ");
        out.push_str(&task.title);
        out.push('\n');

        if self.include_metadata {
            out.push_str("- State: ");
            out.push_str(&task.state);
            out.push('\n');
            if !task.prior.is_empty() {
                out.push_str("- Prior: ");
                out.push_str(&fmt_prior_list(&task.prior));
                out.push('\n');
            }
            if !task.provides.is_empty() {
                out.push_str("**Provides:** ");
                out.push_str(&task.provides.join(", "));
                out.push('\n');
            }
            if !task.consumes.is_empty() {
                out.push_str("**Consumes:** ");
                out.push_str(
                    &task
                        .consumes
                        .iter()
                        .map(|export| format!("{}:{}", export.task, export.name))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push('\n');
            }
            if !task.excludes.is_empty() {
                out.push_str("**Excludes:** ");
                out.push_str(
                    &task
                        .excludes
                        .iter()
                        .map(|entry| entry.authored())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push('\n');
            }
            if let Some(ref assignee) = task.assignee {
                out.push_str("- Assignee: ");
                out.push_str(assignee);
                out.push('\n');
            }
        }

        let content = task.content.trim();
        if self.include_content && !content.is_empty() {
            out.push('\n');
            out.push_str(content);
            out.push('\n');
        }

        out.push('\n');

        for child in &task.children {
            self.render_node(child, level + 1, out);
        }
    }
}

/// Convenience: render rhei to GitHub issues-style Markdown with all sections enabled.
pub fn to_github_markdown(rhei: &rhei_core::ast::Rhei) -> String {
    GithubIssuesOutput { include_content: true, include_metadata: true }.to_markdown(rhei)
}
