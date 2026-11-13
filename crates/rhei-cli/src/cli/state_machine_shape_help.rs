// A state machine that does not load: the report that names it, and what its
// help says to do before the re-check takes over.

/// Turn a state machine load failure into a diagnostic that says what to fix
/// and where. §FS-rhei-errors.1.2 §FS-rhei-errors.1.2.2
fn state_machine_load_report(path: &Path, err: rhei_validator::StateMachineLoadError) -> Report {
    let quoted = shell_quote(&path.display().to_string());
    // A rendered machine is named as its template's rendering; one that is gone is
    // fixed in the template and instantiated again. §FS-rhei-errors.4.1
    let origin = rendered_origin(path);
    let discarded = origin.as_ref().is_some_and(|origin| origin.discarded);
    let rendering = origin.as_ref().map(|origin| format!(", {}", origin.rendering));
    let shown = format!("'{}'{}", path.display(), rendering.unwrap_or_default());
    let subject =
        if discarded { "the rendering".to_owned() } else { format!("'{}'", path.display()) };
    let fix = |what: String| state_machine_recheck_help(&what, &quoted, discarded);
    match err {
        rhei_validator::StateMachineLoadError::Io(err) => {
            file_io_report(path, "failed to read state machine", err)
        }
        rhei_validator::StateMachineLoadError::Yaml(err) => {
            // serde_yaml's message leaves out a mark at the very start of the file, so a
            // failure at the first character has no position above. §FS-rhei-errors.1.2.2
            let printed = err.location().is_some_and(|at| (at.line(), at.column()) != (1, 1));
            let at = if printed { " at the position above" } else { "" };
            // The position is the rendering's, never the template file's. §FS-rhei-errors.4.1
            let what = if discarded {
                format!("{subject} is not valid YAML{at}. Fix the syntax")
            } else {
                format!("{subject} is not valid YAML. Fix the syntax{at}")
            };
            miette!(help = fix(what), "failed to parse state machine {shown}: {err}")
        }
        rhei_validator::StateMachineLoadError::Shape { error, fault } => {
            let cause = fault.cause(&error);
            miette!(
                help = fix(state_machine_shape_help(&subject, fault)),
                "failed to parse state machine {shown}: {cause}"
            )
        }
        rhei_validator::StateMachineLoadError::Invalid(message) => {
            let what = if discarded {
                "make the state definition above valid".to_owned()
            } else {
                format!("edit {subject} so the state definition above is valid")
            };
            miette!(help = fix(what), "invalid state machine {shown}: {message}")
        }
    }
}

/// How a machine's load failure ends: re-check the file it names, or, for a
/// rendering that is gone, fix the template and instantiate again.
/// §FS-rhei-errors.1.2 §FS-rhei-errors.4.1
fn state_machine_recheck_help(what: &str, quoted: &str, discarded: bool) -> String {
    if discarded {
        format!("{what} {RENDERING_RETRY}")
    } else {
        format!("{what}, then re-check it with: rhei states --state-machine {quoted}")
    }
}

/// What to do about a state machine the YAML scan accepts but rhei cannot load,
/// ending where the re-check command takes over. `subject` names the file as the
/// help speaks of it: quoted, or as a rendering. §FS-rhei-errors.1.2.2
fn state_machine_shape_help(
    subject: &str,
    fault: rhei_validator::StateMachineShapeFault,
) -> String {
    use rhei_validator::StateMachineShapeFault as Fault;
    match fault {
        Fault::MissingTopLevelField(field) => format!(
            "{subject} is valid YAML but is missing the required top-level key `{key}`. Add a \
             top-level `{key}:` holding {shape}",
            key = field.key,
            shape = field.shape
        ),
        Fault::NoTopLevelMapping => {
            let keys = rhei_validator::REQUIRED_TOP_LEVEL_FIELDS
                .map(|field| format!("`{}` ({})", field.key, field.shape))
                .join(", ");
            format!(
                "{subject} is valid YAML but holds no mapping, which a state machine must be. \
                 Write a mapping with the keys {keys}"
            )
        }
        Fault::Other => format!(
            "{subject} is valid YAML but is not the shape of a state machine. Correct what the \
             message above names"
        ),
        Fault::SeveralDocuments => format!(
            "{subject} is valid YAML but a state machine must hold one document. Keep one and \
             remove the others"
        ),
        Fault::Unreadable => format!(
            "{subject} cannot be read as a state machine. Correct what the message above names"
        ),
    }
}
