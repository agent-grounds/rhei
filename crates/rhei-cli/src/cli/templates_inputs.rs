    #[derive(Debug, Default)]
    struct ParsedTemplateInputArgs {
        positional_values: Vec<(String, String)>,
        assignments: Vec<(String, String)>,
    }

    fn parse_template_input_args(
        manifest: &TemplateManifest,
        input_args: &[String],
    ) -> MietteResult<ParsedTemplateInputArgs> {
        let ident = Regex::new(r"^[A-Za-z][A-Za-z0-9_-]*$")
            .expect("template identifier regex should be valid");
        let declared_inputs =
            manifest.inputs.iter().map(|input| input.name.as_str()).collect::<HashSet<_>>();
        let mut positional_values = Vec::new();
        let mut assignments = Vec::new();

        for value in input_args {
            if let Some((key, rhs)) = value.split_once('=') {
                if ident.is_match(key) {
                    if !declared_inputs.contains(key) {
                        // §FS-rhei-errors.1.3
                        let names =
                            manifest.inputs.iter().map(|i| i.name.clone()).collect::<Vec<_>>();
                        return Err(miette!(
                            help = format!(
                                "{}List every input with: rhei instantiate {} --list-inputs",
                                did_you_mean(key, &names)
                                    .map(|hint| format!("{hint} "))
                                    .unwrap_or_default(),
                                manifest.name
                            ),
                            "template '{}' has no input named '{}'",
                            manifest.name,
                            key
                        ));
                    }
                    assignments.push((key.to_string(), rhs.to_string()));
                    continue;
                }
            }

            positional_values.push(value.clone());
        }

        let positional_values = map_template_positional_inputs(manifest, &positional_values)?;
        Ok(ParsedTemplateInputArgs { positional_values, assignments })
    }

    fn map_template_positional_inputs(
        manifest: &TemplateManifest,
        values: &[String],
    ) -> MietteResult<Vec<(String, String)>> {
        if values.is_empty() {
            return Ok(Vec::new());
        }

        let positional_inputs = manifest
            .inputs
            .iter()
            .filter_map(|input| input.positional.map(|index| (index, input)))
            .collect::<BTreeMap<_, _>>();

        if !positional_inputs.is_empty() {
            let mut mapped = Vec::new();
            for (idx, value) in values.iter().enumerate() {
                let position = idx + 1;
                let Some(input) = positional_inputs.get(&position) else {
                    return Err(miette!(
                        help = format!(
                            "this template takes {} positional value(s). Name the rest \
                             explicitly as KEY=VALUE — see: rhei instantiate {} --list-inputs",
                            positional_inputs.len(),
                            manifest.name
                        ),
                        "template '{}' has no positional slot {}",
                        manifest.name,
                        position
                    ));
                };
                mapped.push((input.name.clone(), value.clone()));
            }
            return Ok(mapped);
        }

        let required =
            manifest.inputs.iter().filter(|input| input.is_required()).collect::<Vec<_>>();
        if required.len() == 1 && values.len() == 1 {
            return Ok(vec![(required[0].name.clone(), values[0].clone())]);
        }

        // Every input here is optional or defaulted, so there is no list of
        // names to hand back; the naming rule still is.
        let named = required
            .iter()
            .map(|input| format!("{}=<value>", input.name))
            .collect::<Vec<_>>()
            .join(" ");
        let lead = if named.is_empty() {
            "supply values as KEY=VALUE".to_string()
        } else {
            format!("name each value: {named}")
        };
        Err(miette!(
            help = format!(
                "{lead}. List every input with: rhei instantiate {} --list-inputs",
                manifest.name
            ),
            "template '{}' does not accept positional inputs",
            manifest.name
        ))
    }

    /// Choose a correction by scanning all documents before offering document-count
    /// help. Keep syntax guidance independent of coordinates: the retained loading
    /// cause may report only multiple documents. §FS-rhei-errors.1.2.3
    fn template_values_load_correction(raw: &str) -> &'static str {
        let mut documents = 0_usize;
        for document in serde_yaml::Deserializer::from_str(raw) {
            if serde::de::IgnoredAny::deserialize(document).is_err() {
                return "Fix the syntax in the values file.";
            }
            documents += 1;
        }
        if documents > 1 {
            "Use one YAML document in each --values file; combine its input mappings into that document."
        } else {
            "Correct the loading problem reported above, such as a number out of range or a duplicate key."
        }
    }

    /// Preserve values-file loading and shape checks, selecting help from the text
    /// rather than the words of the original loading cause. §FS-rhei-errors.1.2.3
    fn load_template_values_file(path: &Path) -> MietteResult<BTreeMap<String, YamlValue>> {
        let raw = fs::read_to_string(path)
            .map_err(|err| file_io_report(path, "failed to read values file", err))?;
        // §REQ-cross-platform.9
        let raw = rhei_core::source::without_byte_order_mark(&raw);
        if raw.trim().is_empty() {
            return Ok(BTreeMap::new());
        }

        let value: YamlValue = serde_yaml::from_str(raw)
            .map_err(|err| {
                miette!(
                    help = format!(
                        "a --values file is a YAML or JSON object mapping input names to \
                         values. {}",
                        template_values_load_correction(&raw)
                    ),
                    "failed to parse values file '{}': {err}",
                    path.display()
                )
            })?;
        let mapping = match value {
            YamlValue::Mapping(mapping) => mapping,
            _ => {
                return Err(miette!(
                    help = "write one `input_name: value` pair per line at the top level of \
                            the file.",
                    "values file '{}' must contain a YAML or JSON object at the top level",
                    path.display()
                ))
            }
        };

        let mut values = BTreeMap::new();
        for (key, value) in mapping {
            let Some(key) = key.as_str() else {
                return Err(miette!(
                    help = "every top-level key must be an input name, written as plain text.",
                    "values file '{}' contains a non-string key",
                    path.display()
                ));
            };
            values.insert(key.to_string(), value);
        }

        Ok(values)
    }

    fn parse_assignment(value: &str, flag_name: &str) -> MietteResult<(String, String)> {
        let Some((key, value)) = value.split_once('=') else {
            return Err(miette!(
                help = format!("write it as {} <input>=<value>", flag_name),
                "{} expects KEY=VALUE, got '{}'",
                flag_name,
                value
            ));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(miette!(
                help = format!("name the input before the '=': {} <input>=<value>", flag_name),
                "{} expects a non-empty key",
                flag_name
            ));
        }
        Ok((key.to_string(), value.to_string()))
    }

    fn compile_full_match_regex(pattern: &str) -> Result<Regex> {
        Regex::new(&format!(r"\A(?:{})\z", pattern)).context("compile regex")
    }

    fn coerce_template_input_value(
        input: &TemplateInputDef,
        raw: &YamlValue,
        cwd: &Path,
        from_default: bool,
    ) -> MietteResult<serde_json::Value> {
        coerce_template_value(&input.name, &input.schema, raw, cwd, from_default)
    }

    fn coerce_template_value(
        label: &str,
        schema: &TemplateValueSchema,
        raw: &YamlValue,
        cwd: &Path,
        from_default: bool,
    ) -> MietteResult<serde_json::Value> {
        let source = if from_default { "default value" } else { "input value" };

        let rendered = match schema.value_type {
            TemplateInputType::String => match raw {
                YamlValue::Null => serde_json::Value::String(String::new()),
                YamlValue::String(value) => serde_json::Value::String(value.clone()),
                _ => {
                    return Err(miette!(
                        help = format!("supply it as text, e.g. {label}='some text'"),
                        "{} for '{}' must be a string",
                        source,
                        label
                    ))
                }
            },
            TemplateInputType::Number => match raw {
                YamlValue::Number(value) => serde_json::to_value(value)
                    .map_err(|err| miette!(
                        help = template_manifest_help(),
                        "failed to serialize number for '{}': {err}", label
                    ))?,
                YamlValue::String(value) => {
                    let trimmed = value.trim();
                    let number_re = Regex::new(r"^-?\d+(?:\.\d+)?$")
                        .expect("number validation regex should be valid");
                    if !number_re.is_match(trimmed) {
                        return Err(miette!(
                            help = format!("supply a plain number, e.g. {label}=3"),
                            "{} for '{}' must be a number, got '{}'",
                            source,
                            label,
                            trimmed
                        ));
                    }
                    let parsed: YamlValue = serde_yaml::from_str(trimmed).map_err(|err| {
                        miette!(
                            help = template_manifest_help(),
                            "{} for '{}' must be a number: {err}", source, label
                        )
                    })?;
                    serde_json::to_value(parsed).map_err(|err| {
                        miette!(
                            help = template_manifest_help(),
                            "failed to serialize number for '{}': {err}", label
                        )
                    })?
                }
                _ => {
                    return Err(miette!(
                        help = format!("supply a plain number, e.g. {label}=3"),
                        "{} for '{}' must be a number",
                        source,
                        label
                    ))
                }
            },
            TemplateInputType::Boolean => match raw {
                YamlValue::Bool(value) => serde_json::Value::Bool(*value),
                YamlValue::String(value) => match value.trim() {
                    "true" => serde_json::Value::Bool(true),
                    "false" => serde_json::Value::Bool(false),
                    other => {
                        return Err(miette!(
                            help = format!("supply {label}=true or {label}=false"),
                            "{} for '{}' must be true or false, got '{}'",
                            source,
                            label,
                            other
                        ))
                    }
                },
                _ => {
                    return Err(miette!(
                        help = format!("supply {label}=true or {label}=false"),
                        "{} for '{}' must be true or false",
                        source,
                        label
                    ))
                }
            },
            TemplateInputType::Path => match raw {
                YamlValue::String(value) => {
                    if value.is_empty() {
                        return Err(miette!(
                            help = format!("supply a path, e.g. {label}=./some/dir"),
                            "{} for '{}' must not be empty",
                            source,
                            label
                        ));
                    }
                    let path = PathBuf::from(value);
                    let resolved = if path.is_absolute() { path } else { cwd.join(path) };
                    if !from_default && !resolved.exists() {
                        // §FS-rhei-errors.6: a path input is resolved against the
                        // caller's cwd, so say which directory it was resolved from.
                        return Err(miette!(
                            help = format!(
                                "create it first, or point the input somewhere that exists. \
                                 Relative values resolve against {}",
                                cwd.display()
                            ),
                            "{} for '{}' refers to a path that does not exist: {}",
                            source,
                            label,
                            resolved.display()
                        ));
                    }
                    serde_json::Value::String(resolved.display().to_string())
                }
                _ => {
                    return Err(miette!(
                        help = format!("supply a path, e.g. {label}=./some/dir"),
                        "{} for '{}' must be a path string",
                        source,
                        label
                    ))
                }
            },
            TemplateInputType::Array => {
                let sequence = parse_template_sequence(label, raw, source)?;
                let item_schema = schema.items.as_deref().ok_or_else(|| {
                    miette!(
                        help = internal_error_help(),
                        "{} for '{}' requires an items schema",
                        source,
                        label
                    )
                })?;
                let mut items = Vec::with_capacity(sequence.len());
                for (idx, item) in sequence.iter().enumerate() {
                    items.push(coerce_template_value(
                        &format!("{label}[{idx}]"),
                        item_schema,
                        item,
                        cwd,
                        from_default,
                    )?);
                }
                serde_json::Value::Array(items)
            }
            TemplateInputType::Object => {
                let mapping = parse_template_mapping(label, raw, source)?;
                let mut object = serde_json::Map::new();
                for (key_value, value) in mapping {
                    let Some(key) = key_value.as_str() else {
                        return Err(miette!(
                            help = format!(
                                "write {label} as an object whose keys are property names, \
                                 e.g. {label}='{{key: value}}'"
                            ),
                            "{} for '{}' contains a non-string key",
                            source,
                            label
                        ));
                    };
                    let property_schema = schema.properties.get(key).ok_or_else(|| {
                        let known = schema.properties.keys().cloned().collect::<Vec<_>>();
                        miette!(
                            help = did_you_mean(key, &known)
                                .unwrap_or_else(|| format!("'{label}' declares no properties")),
                            "{} for '{}' contains unknown property '{}'",
                            source,
                            label,
                            key
                        )
                    })?;
                    object.insert(
                        key.to_string(),
                        coerce_template_value(key, property_schema, &value, cwd, from_default)?,
                    );
                }
                for (property, property_schema) in &schema.properties {
                    if object.contains_key(property) {
                        continue;
                    }
                    if let Some(default) = property_schema.default.as_ref() {
                        object.insert(
                            property.clone(),
                            coerce_template_value(property, property_schema, default, cwd, true)?,
                        );
                    } else if property_schema.is_required() {
                        return Err(miette!(
                            help = format!(
                                "add it to the value, e.g. {label}='{{{property}: <value>}}'"
                            ),
                            "{} for '{}' is missing required property '{}'",
                            source,
                            label,
                            property
                        ));
                    } else {
                        object.insert(property.clone(), empty_template_value(property_schema));
                    }
                }
                serde_json::Value::Object(object)
            }
        };

        Ok(rendered)
    }

    fn parse_template_sequence(
        label: &str,
        raw: &YamlValue,
        source: &str,
    ) -> MietteResult<Vec<YamlValue>> {
        match parse_structured_template_value(raw, "array", label, source)? {
            YamlValue::Sequence(values) => Ok(values),
            _ => Err(miette!(
                help = format!("write it as a YAML or JSON list, e.g. {label}='[one, two]'"),
                "{} for '{}' must be an array",
                source,
                label
            )),
        }
    }

    fn parse_template_mapping(
        label: &str,
        raw: &YamlValue,
        source: &str,
    ) -> MietteResult<YamlMapping> {
        match parse_structured_template_value(raw, "object", label, source)? {
            YamlValue::Mapping(values) => Ok(values),
            _ => Err(miette!(
                help = format!("write it as a YAML or JSON object, e.g. {label}='{{key: value}}'"),
                "{} for '{}' must be an object",
                source,
                label
            )),
        }
    }

    fn parse_structured_template_value(
        raw: &YamlValue,
        expected: &str,
        label: &str,
        source: &str,
    ) -> MietteResult<YamlValue> {
        match raw {
            YamlValue::String(text) => serde_yaml::from_str::<YamlValue>(text).map_err(|err| {
                miette!(
                    help = format!(
                        "{label} takes a YAML or JSON {expected} on the command line, e.g. \
                         {}",
                        if expected == "array" {
                            format!("{label}='[one, two]'")
                        } else {
                            format!("{label}='{{key: value}}'")
                        }
                    ),
                    "{} for '{}' must be valid YAML or JSON {} syntax: {err}",
                    source,
                    label,
                    expected
                )
            }),
            other => Ok(other.clone()),
        }
    }

    fn empty_template_value(schema: &TemplateValueSchema) -> serde_json::Value {
        match schema.value_type {
            TemplateInputType::String | TemplateInputType::Path => {
                serde_json::Value::String(String::new())
            }
            TemplateInputType::Number | TemplateInputType::Boolean => serde_json::Value::Null,
            TemplateInputType::Array => serde_json::Value::Array(Vec::new()),
            TemplateInputType::Object => serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    fn scalar_template_value_as_string(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::Null => Some(String::new()),
            serde_json::Value::Bool(value) => Some(value.to_string()),
            serde_json::Value::Number(value) => Some(value.to_string()),
            serde_json::Value::String(value) => Some(value.clone()),
            _ => None,
        }
    }
