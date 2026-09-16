// Resolution and availability checks for the plan-level files exchanged by
// `**Provides:**` and `**Consumes:**`.
//
// Its own part because prompt composition and terminal transitions must agree
// on what one export is, while neither should own the other's policy.

// §AR-source-file-size.3 §FS-rhei-plan-language.3.12

#[derive(Debug)]
enum TaskExportContent {
    Available(String),
    Unavailable {
        relative: String,
        path: PathBuf,
        legacy: Option<(String, PathBuf)>,
    },
}

/// Workspace-relative location of one task export.
// §FS-rhei-plan-language.3.12
fn task_export_relative_path(task_id: &str, name: &str) -> String {
    format!("runtime/exports/{task_id}/{name}.md")
}

/// Resolve an already-derived export path without permitting it to climb out
/// of the producer's execution root.
// §FS-rhei-plan-language.3.12.2
fn resolve_task_export_relative_path(
    root: &Path,
    relative: String,
) -> MietteResult<(String, PathBuf)> {
    if artifact_relative_path_escapes_root(&relative) {
        return Err(miette!(
            help = "task export paths are execution-root-relative; use a path-safe task id and export name",
            "Task export path '{}' escapes the producer execution root",
            relative
        ));
    }
    Ok((relative.clone(), root.join(relative)))
}

/// Resolve one export beneath the execution root of the task that publishes
/// it. Callers choose the producer root before entering this helper.
// §FS-rhei-plan-language.3.12.2 §FS-rhei-panta.6.1
fn resolve_task_export_path(
    root: &Path,
    task_id: &str,
    name: &str,
) -> MietteResult<(String, PathBuf)> {
    resolve_task_export_relative_path(root, task_export_relative_path(task_id, name))
}

/// Read one export as nonblank text and retain the paths needed for a batched
/// repair diagnostic when it is absent or blank.
// §FS-rhei-plan-language.3.12.2
fn load_task_export(root: &Path, task_id: &str, name: &str) -> MietteResult<TaskExportContent> {
    let (relative, path) = resolve_task_export_path(root, task_id, name)?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(file_io_report(&path, "failed to read task export", err)),
    };
    if !content.trim().is_empty() {
        return Ok(TaskExportContent::Available(content.trim().to_string()));
    }

    let local_id = rhei_local_id_str(task_id);
    let legacy = if local_id != task_id {
        let (legacy_relative, legacy_path) = resolve_task_export_path(root, local_id, name)?;
        legacy_path.exists().then_some((legacy_relative, legacy_path))
    } else {
        None
    };
    Ok(TaskExportContent::Unavailable { relative, path, legacy })
}

fn unavailable_task_export_line(
    reference: &str,
    relative: &str,
    path: &Path,
    legacy: Option<&(String, PathBuf)>,
) -> String {
    let legacy_hint = legacy
        .map(|(_, legacy_path)| {
            format!(
                "; rename the pre-qualification export at '{}'",
                legacy_path.display()
            )
        })
        .unwrap_or_default();
    format!("- {reference} ({relative}; looked for at {}{legacy_hint})", path.display())
}

/// Refuse successful terminal entry until each export promised by this task
/// contains non-whitespace text. The caller places this after source outputs
/// and effective-target resolution and before every terminal effect.
// §FS-rhei-plan-language.3.12.3 §FS-rhei-transition-cmd.3.3
fn ensure_declared_task_exports_exist(
    root: &Path,
    task: &rhei_core::ast::Task,
    qualified_id: &str,
    terminal_state: &str,
) -> MietteResult<()> {
    let mut unavailable = Vec::new();
    for name in &task.provides {
        if let TaskExportContent::Unavailable { relative, path, legacy } =
            load_task_export(root, qualified_id, name)?
        {
            unavailable.push(unavailable_task_export_line(
                name,
                &relative,
                &path,
                legacy.as_ref(),
            ));
        }
    }
    if unavailable.is_empty() {
        return Ok(());
    }
    Err(miette!(
        help = "write nonblank text to every declared export, then retry the transition",
        "Task {} cannot enter terminal state '{}'.\nMissing or blank declared exports:\n{}",
        qualified_id,
        terminal_state,
        unavailable.join("\n")
    ))
}

#[cfg(test)]
mod task_export_path_tests {
    use super::*;

    /// The shared resolver rejects a derived relative path before joining it
    /// to the producer root. §FS-rhei-plan-language.3.12.2
    #[test]
    fn task_export_resolver_rejects_a_path_that_escapes_the_producer_root() {
        let error = resolve_task_export_relative_path(
            Path::new("producer-root"),
            "../../outside.md".to_string(),
        )
        .expect_err("an escaping export path must be refused");

        assert!(error.to_string().contains("escapes the producer execution root"));
    }
}
