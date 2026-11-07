/// Say what a document that parsed as `raw`, but not into a [`StateMachine`], lacks.
///
/// The deserializer's error has no structured kind, and its words do not separate a
/// `name` missing at the top level from one missing inside an input, so this reads
/// the document rather than the message. A required key being absent is not enough
/// either: an entry that fails is reported before the absent key is, so the absent
/// key is the refusal only when the machine parses once it is there. Otherwise the
/// help would correct a key the headline never named. §FS-rhei-errors.1.2.2
fn state_machine_shape_fault(raw: &serde_yaml::Value) -> StateMachineShapeFault {
    // A tag on the document does not stop it deserializing as the mapping beneath.
    let mut document = raw;
    while let serde_yaml::Value::Tagged(tagged) = document {
        document = &tagged.value;
    }
    let serde_yaml::Value::Mapping(top) = document else {
        return StateMachineShapeFault::NoTopLevelMapping;
    };
    let absent = REQUIRED_TOP_LEVEL_FIELDS.iter().find(|field| !top.contains_key(field.key));
    match absent {
        Some(field) if parses_with_required_fields_supplied(top) => {
            StateMachineShapeFault::MissingTopLevelField(*field)
        }
        _ => StateMachineShapeFault::Other,
    }
}

/// Whether `top` parses into a [`StateMachine`] once every required key it lacks is
/// supplied. Back through text, because a plain `3` reads as a string from text but
/// not from a [`serde_yaml::Value`], and the answer has to be the text parser's.
fn parses_with_required_fields_supplied(top: &serde_yaml::Mapping) -> bool {
    let Ok(mut whole) = serde_yaml::from_str::<serde_yaml::Mapping>("{name: m, version: 1, states: {}}")
    else {
        return false;
    };
    whole.extend(top.clone());
    serde_yaml::to_string(&whole)
        .is_ok_and(|text| serde_yaml::from_str::<StateMachine>(&text).is_ok())
}
