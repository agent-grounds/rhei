    // Bind schemas use existing input types. §FS-rhei-library.2
    fn compatible_input_schema(left: &TemplateValueSchema, right: &TemplateValueSchema) -> bool {
        left.value_type == right.value_type
            && left.validate == right.validate
            && left.format == right.format
            && match (left.items.as_deref(), right.items.as_deref()) {
                (Some(left), Some(right)) => compatible_input_schema(left, right),
                (None, None) => true,
                _ => false,
            }
            && left.properties.len() == right.properties.len()
            && left.properties.iter().all(|(name, left)| {
                right
                    .properties
                    .get(name)
                    .is_some_and(|right| compatible_input_schema(left, right))
            })
    }
