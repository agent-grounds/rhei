/// What to do about a state machine the YAML scan accepts but rhei cannot load,
/// ending where the re-check command takes over. §FS-rhei-errors.1.2.2
fn state_machine_shape_help(path: &Path, fault: rhei_validator::StateMachineShapeFault) -> String {
    use rhei_validator::StateMachineShapeFault as Fault;
    let path = path.display();
    match fault {
        Fault::MissingTopLevelField(field) => format!(
            "'{path}' is valid YAML but is missing the required top-level key `{key}`. Add a \
             top-level `{key}:` holding {shape}",
            key = field.key,
            shape = field.shape
        ),
        Fault::NoTopLevelMapping => {
            let keys = rhei_validator::REQUIRED_TOP_LEVEL_FIELDS
                .map(|field| format!("`{}` ({})", field.key, field.shape))
                .join(", ");
            format!(
                "'{path}' is valid YAML but holds no mapping, which a state machine must be. \
                 Write a mapping with the keys {keys}"
            )
        }
        Fault::Other => format!(
            "'{path}' is valid YAML but is not the shape of a state machine. Correct what the \
             message above names"
        ),
        Fault::SeveralDocuments => format!(
            "'{path}' is valid YAML but a state machine must hold one document. Keep one and \
             remove the others"
        ),
        Fault::Unreadable => format!(
            "'{path}' cannot be read as a state machine. Correct what the message above names"
        ),
    }
}
