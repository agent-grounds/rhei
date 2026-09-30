    // What an input's declared schema has to be before anything is rendered:
    // types, defaults, patterns, formats, and the nested `items`/`properties`
    // the two container types take.
    //
    // Its own part because it reads one manifest entry at a time and knows
    // nothing about where the template was found.

    // §FS-rhei-templates.3

    fn validate_template_value_schema(
        template_name: &str,
        label: &str,
        schema: &TemplateValueSchema,
    ) -> MietteResult<()> {
        if let Some(pattern) = schema.validate.as_deref() {
            if matches!(schema.value_type, TemplateInputType::Array | TemplateInputType::Object) {
                return Err(miette!(
                    help = template_manifest_help(),
                    "template '{}' input '{}' cannot set validate on {} values",
                    template_name,
                    label,
                    schema.value_type.as_str()
                ));
            }
            let _ = compile_full_match_regex(pattern).map_err(|err| {
                miette!(
                    help = template_manifest_help(),
                    "template '{}' input '{}' has invalid validate regex: {err}",
                    template_name,
                    label
                )
            })?;
        }

        if let Some(format) = schema.format {
            if matches!(schema.value_type, TemplateInputType::Array | TemplateInputType::Object) {
                return Err(miette!(
                    help = format!(
                        "move `format: {}` onto the scalar it applies to — the array's `items` \
                         entry or the object property — instead of the {} itself.",
                        format.as_str(),
                        schema.value_type.as_str()
                    ),
                    "template '{}' input '{}' cannot set format on {} values",
                    template_name,
                    label,
                    schema.value_type.as_str()
                ));
            }
        }

        match schema.value_type {
            TemplateInputType::Array => {
                let Some(items) = schema.items.as_deref() else {
                    return Err(miette!(
                        help = template_manifest_help(),
                        "template '{}' input '{}' with type array must declare items",
                        template_name,
                        label
                    ));
                };
                if !schema.properties.is_empty() {
                    return Err(miette!(
                        help = template_manifest_help(),
                        "template '{}' input '{}' with type array cannot declare properties",
                        template_name,
                        label
                    ));
                }
                validate_template_value_schema(template_name, label, items)?;
            }
            TemplateInputType::Object => {
                if schema.items.is_some() {
                    return Err(miette!(
                        help = template_manifest_help(),
                        "template '{}' input '{}' with type object cannot declare items",
                        template_name,
                        label
                    ));
                }
                for (property, property_schema) in &schema.properties {
                    validate_template_value_schema(
                        template_name,
                        &format!("{label}.{property}"),
                        property_schema,
                    )?;
                }
            }
            _ => {
                if schema.items.is_some() {
                    return Err(miette!(
                        help = template_manifest_help(),
                        "template '{}' input '{}' with type {} cannot declare items",
                        template_name,
                        label,
                        schema.value_type.as_str()
                    ));
                }
                if !schema.properties.is_empty() {
                    return Err(miette!(
                        help = template_manifest_help(),
                        "template '{}' input '{}' with type {} cannot declare properties",
                        template_name,
                        label,
                        schema.value_type.as_str()
                    ));
                }
            }
        }

        Ok(())
    }
