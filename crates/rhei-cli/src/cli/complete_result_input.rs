// Loading alternate result sources belongs in the command frontend: all input
// errors must win before completion resolves or reads a plan.

/// Resolve the result message selected for `rhei complete`.
///
/// Clap enforces exactly one source. Its empty default for `result` keeps the
/// legacy declaration field concrete while `result_file` is selected.
// §FS-rhei-complete.2.2 §FS-rhei-complete.4
fn resolve_complete_result(result: String, result_file: Option<&Path>) -> MietteResult<String> {
    let message = match result_file {
        Some(path) if path.as_os_str() == "-" => {
            let stdin = std::io::stdin();
            read_complete_result(stdin.lock(), "standard input")?
        }
        Some(path) => {
            let file = fs::File::open(path).map_err(|err| complete_result_read_error(path, err))?;
            read_complete_result(file, &format!("'{}'", path.display()))?
        }
        None => result,
    };

    if result_file.is_none() {
        require_non_blank_result(Some(&message), "complete")?;
    } else if message.trim().is_empty() {
        return Err(miette!(
            help = "pass a non-empty message with `--result`, or read one with `--result-file <path>` (`-` for standard input).",
            "completion result carries no message"
        ));
    }
    Ok(message)
}

/// Read one complete UTF-8 message. Keeping the reader injectable lets a unit
/// test prove that an actual stdin read failure takes this same error path.
// §FS-rhei-complete.2.2
fn read_complete_result(mut reader: impl Read, source: &str) -> MietteResult<String> {
    let mut message = String::new();
    reader.read_to_string(&mut message).map_err(|err| {
        miette!(
            help = "the completion result must be readable UTF-8; check the source and try again.",
            "failed to read the completion result from {source}: {err}"
        )
    })?;
    Ok(message)
}

// §FS-rhei-complete.2.2
fn complete_result_read_error(path: &Path, err: std::io::Error) -> Report {
    miette!(
        help = "check that the path exists and is a readable UTF-8 file, or use `--result-file -` to read standard input.",
        "failed to read the completion result from '{}': {err}",
        path.display()
    )
}
