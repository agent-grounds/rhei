/// What to do about a state machine whose text parses but has the wrong shape,
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
    }
}
