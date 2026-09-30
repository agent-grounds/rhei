    // Shell completion for `rhei instantiate` and `rhei templates`: which
    // template, which input, and what a value for it could be.
    //
    // Its own part because completion reads the manifest and answers about a
    // half-typed command line, which is a different job from listing templates
    // or rendering one in detail.

    // §FS-rhei-templates.6.1

    pub(super) fn complete_template_reference(current: &OsStr) -> Vec<CompletionCandidate> {
        let Some(current_str) = current.to_str() else {
            return PathCompleter::dir().complete(current);
        };

        if template_reference_is_path(current_str) {
            return PathCompleter::dir().complete(current);
        }

        let Ok(templates) = discover_templates(TemplateSourceFilter::All) else {
            return Vec::new();
        };

        templates
            .into_iter()
            .filter(|template| template.manifest.name.starts_with(current_str))
            .map(|template| {
                let help =
                    format!("{} ({})", template.manifest.description, template.source.as_str());
                CompletionCandidate::new(template.manifest.name).help(Some(help.into()))
            })
            .collect()
    }

    pub(super) fn complete_template_input_arg(current: &OsStr) -> Vec<CompletionCandidate> {
        let Some((manifest, input_args)) = completion_template_context() else {
            return Vec::new();
        };
        complete_template_input_value(&manifest, &input_args, current, false)
    }

    pub(super) fn complete_template_set_value(current: &OsStr) -> Vec<CompletionCandidate> {
        let Some((manifest, input_args)) = completion_template_context() else {
            return Vec::new();
        };
        complete_template_assignment(&manifest, &input_args, current, false)
    }

    pub(super) fn complete_template_set_file(current: &OsStr) -> Vec<CompletionCandidate> {
        let Some((manifest, input_args)) = completion_template_context() else {
            return Vec::new();
        };
        complete_template_assignment(&manifest, &input_args, current, true)
    }

    fn complete_template_input_value(
        manifest: &TemplateManifest,
        prior_input_args: &[String],
        current: &OsStr,
        set_file: bool,
    ) -> Vec<CompletionCandidate> {
        let current_str = current.to_string_lossy();
        if current_str.contains('=') {
            return complete_template_assignment(manifest, prior_input_args, current, set_file);
        }

        let mut candidates = Vec::new();
        if let Some(input) = next_positional_input(manifest, prior_input_args) {
            candidates.extend(complete_template_value_for_input(input, current, None, false));
        }
        candidates.extend(complete_template_assignment_keys(manifest, prior_input_args, current));
        candidates
    }

    fn complete_template_assignment(
        manifest: &TemplateManifest,
        prior_input_args: &[String],
        current: &OsStr,
        set_file: bool,
    ) -> Vec<CompletionCandidate> {
        let current_str = current.to_string_lossy();
        let Some((key, value_prefix)) = current_str.split_once('=') else {
            return complete_template_assignment_keys(manifest, prior_input_args, current);
        };

        let Some(input) = manifest.inputs.iter().find(|input| input.name == key) else {
            return Vec::new();
        };
        complete_template_value_for_input(input, OsStr::new(value_prefix), Some(key), set_file)
    }

    fn complete_template_assignment_keys(
        manifest: &TemplateManifest,
        prior_input_args: &[String],
        current: &OsStr,
    ) -> Vec<CompletionCandidate> {
        let prefix = current.to_string_lossy();
        let supplied = supplied_template_input_keys(manifest, prior_input_args);
        manifest
            .inputs
            .iter()
            .filter(|input| !supplied.contains(input.name.as_str()))
            .filter(|input| input.name.starts_with(prefix.as_ref()))
            .map(|input| {
                CompletionCandidate::new(format!("{}=", input.name))
                    .help(Some(template_input_help(input).into()))
            })
            .collect()
    }

    fn complete_template_value_for_input(
        input: &TemplateInputDef,
        current: &OsStr,
        assignment_key: Option<&str>,
        set_file: bool,
    ) -> Vec<CompletionCandidate> {
        let mut candidates = if set_file {
            PathCompleter::file().complete(current)
        } else {
            match input.value_type() {
                TemplateInputType::Path => PathCompleter::any().complete(current),
                TemplateInputType::Boolean => static_completion(
                    current,
                    &[("true", "Boolean true"), ("false", "Boolean false")],
                ),
                TemplateInputType::Array => static_completion(
                    current,
                    &[("[]", "Empty array"), ("[item]", "Array snippet")],
                ),
                TemplateInputType::Object => static_completion(current, &[("{}", "Empty object")]),
                TemplateInputType::String | TemplateInputType::Number => Vec::new(),
            }
        };

        if let Some(key) = assignment_key {
            let prefix = format!("{key}=");
            candidates =
                candidates.into_iter().map(|candidate| candidate.add_prefix(&prefix)).collect();
        }
        candidates
    }

    fn template_input_help(input: &TemplateInputDef) -> String {
        let requirement = if let Some(default) = input.schema.default.as_ref() {
            format!("default {}", format_version(default))
        } else if input.is_required() {
            "required".to_string()
        } else {
            "optional".to_string()
        };
        let positional =
            input.positional.map(|index| format!(", positional {index}")).unwrap_or_default();
        format!(
            "{}, {}{} - {}",
            input.value_type().as_str(),
            requirement,
            positional,
            input.description
        )
    }

    fn next_positional_input<'a>(
        manifest: &'a TemplateManifest,
        prior_input_args: &[String],
    ) -> Option<&'a TemplateInputDef> {
        let positional_count = prior_input_args
            .iter()
            .filter(|value| !template_input_arg_is_assignment(manifest, value))
            .count();
        let next_position = positional_count + 1;
        if let Some(input) =
            manifest.inputs.iter().find(|input| input.positional == Some(next_position))
        {
            return Some(input);
        }
        if manifest.inputs.iter().all(|input| input.positional.is_none()) && positional_count == 0 {
            let required =
                manifest.inputs.iter().filter(|input| input.is_required()).collect::<Vec<_>>();
            if required.len() == 1 {
                return Some(required[0]);
            }
        }
        None
    }

    fn supplied_template_input_keys<'a>(
        manifest: &'a TemplateManifest,
        prior_input_args: &'a [String],
    ) -> HashSet<&'a str> {
        let mut supplied = HashSet::new();
        let mut positional_index = 1;
        for value in prior_input_args {
            if let Some((key, _)) = value.split_once('=') {
                if manifest.inputs.iter().any(|input| input.name == key) {
                    supplied.insert(key);
                    continue;
                }
            }
            if let Some(input) =
                manifest.inputs.iter().find(|input| input.positional == Some(positional_index))
            {
                supplied.insert(input.name.as_str());
                positional_index += 1;
            } else if manifest.inputs.iter().all(|input| input.positional.is_none()) {
                let required =
                    manifest.inputs.iter().filter(|input| input.is_required()).collect::<Vec<_>>();
                if required.len() == 1 && positional_index == 1 {
                    supplied.insert(required[0].name.as_str());
                    positional_index += 1;
                }
            }
        }
        supplied
    }

    fn template_input_arg_is_assignment(manifest: &TemplateManifest, value: &str) -> bool {
        value
            .split_once('=')
            .is_some_and(|(key, _)| manifest.inputs.iter().any(|input| input.name == key))
    }

    fn completion_template_context() -> Option<(TemplateManifest, Vec<String>)> {
        let words = completion_words();
        let instantiate_index = words.iter().position(|word| word == "instantiate")?;
        let words = words.get(instantiate_index + 1..)?;
        let before_current = words.get(..words.len().saturating_sub(1)).unwrap_or(words);
        let (template, input_args) = completion_template_and_inputs(before_current)?;
        let resolved = resolve_template_reference(&template).ok()?;
        let manifest = load_template_manifest(resolved.path()).ok()?;
        Some((manifest, input_args))
    }

    fn completion_template_and_inputs(words: &[String]) -> Option<(String, Vec<String>)> {
        let mut template = None;
        let mut input_args = Vec::new();
        let mut expects_value_for: Option<&str> = None;

        for word in words {
            if word.is_empty() {
                break;
            }
            if let Some(option) = expects_value_for.take() {
                if matches!(option, "set" | "set-file") {
                    input_args.push(word.clone());
                }
                continue;
            }
            if let Some(option) = word.strip_prefix("--") {
                if let Some((name, value)) = option.split_once('=') {
                    if matches!(name, "set" | "set-file") {
                        input_args.push(value.to_string());
                    }
                    continue;
                }
                if matches!(option, "set" | "set-file" | "values" | "output") {
                    expects_value_for = Some(option);
                }
                continue;
            }
            if template.is_none() {
                template = Some(word.clone());
            } else {
                input_args.push(word.clone());
            }
        }

        template.map(|template| (template, input_args))
    }
