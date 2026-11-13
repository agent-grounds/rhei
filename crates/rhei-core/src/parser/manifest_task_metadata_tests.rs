//! Manifest metadata belongs to task files, while prose and quoted examples
//! remain content. §FS-rhei-plan-language.1.2 §FS-rhei-plan-language.1.5

use super::{parse_panta_manifest, parse_workspace_index};

const FIELDS: [&str; 3] = ["**State:** pending", "**Prior:** Task 2", "**Assignee:** alice"];
const FRONTMATTER: &str = "---\nmetadata:\n  example: |\n    **State:** pending\n    ```\n---\n\n";

/// Exercise each field without a State line masking the other metadata keys.
/// The second placement follows YAML so error coordinates cannot be relative
/// to a stripped body. §FS-rhei-plan-language.1.2 §FS-rhei-plan-language.1.5
fn assert_metadata_refused(panta: bool) {
    let header = if panta { "# Panta: Project" } else { "# Rhei: WS" };
    let mut accepted = Vec::new();
    for field in FIELDS {
        for (prefix, expected_line) in [
            ("\n### Task 1: Book\n".to_string(), 4),
            (format!("\n{FRONTMATTER}## Notes\n\nSome notes.\n\n### Task 1: Book\n"), 15),
            ("\n".to_string(), 3),
        ] {
            let input = format!("{header}\n{prefix}{field}\n");
            let result = if panta {
                parse_panta_manifest(&input).map(|_| ())
            } else {
                parse_workspace_index(&input).map(|_| ())
            };
            match result {
                Ok(()) => accepted.push(format!("{field} at line {expected_line}")),
                Err(error) => {
                    assert_eq!(error.line, Some(expected_line), "{field}: {error:?}");
                    let message = error.message.to_lowercase();
                    assert!(message.contains("task") && message.contains("file"), "{error:?}");
                    if panta {
                        assert!(
                            message.contains("basin/") || message.contains("rhei"),
                            "Panta needs guidance to a rhei's task files or basin/: {error:?}"
                        );
                    } else {
                        assert!(message.contains("tasks/"), "{error:?}");
                    }
                }
            }
        }
    }
    assert!(accepted.is_empty(), "manifest accepted task metadata: {}", accepted.join(", "));
}

#[test]
#[ignore = "red until #696: workspace index accepts task metadata; implement removes this"]
fn workspace_index_refuses_recognized_task_metadata() {
    assert_metadata_refused(false);
}

#[test]
#[ignore = "red until #696: Panta manifest accepts task metadata; implement removes this"]
fn panta_manifest_refuses_recognized_task_metadata() {
    assert_metadata_refused(true);
}

/// Both manifests share the same fence and frontmatter exemptions, and neither
/// treats a heading alone or an arbitrary bold label as task metadata.
/// §FS-rhei-plan-language.2.1
#[test]
fn manifest_prose_and_quoted_task_metadata_remain_content() {
    let content = "## Notes\n\n### Step 1: Explain\n\n### Task 1: Only a heading\n\n\
                   **Audience:** authors\n\n\
                   ```markdown\n### Task 1: Book\n**State:** pending\n**Prior:** Task 2\n```\n\n\
                   ~~~markdown\n### Task 1: Book\n**Assignee:** alice\n~~~\n";
    let workspace = parse_workspace_index(&format!("# Rhei: WS\n{FRONTMATTER}{content}"))
        .expect("workspace examples are content");
    let panta = parse_panta_manifest(&format!("# Panta: Project\n{FRONTMATTER}{content}"))
        .expect("Panta examples are content");
    assert!(workspace.metadata.is_some());
    assert_eq!(workspace.metadata, panta.metadata);
    assert_eq!(workspace.content_sections, panta.content_sections);
    assert_eq!(workspace.content_sections.len(), 1);
    assert_eq!(
        workspace.content_sections[0].content,
        content.strip_prefix("## Notes\n\n").unwrap().trim_end()
    );
}
