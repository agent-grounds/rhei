    fn parse_template_source_filter(value: &str) -> MietteResult<TemplateSourceFilter> {
        match value.trim().to_ascii_lowercase().as_str() {
            "project" => Ok(TemplateSourceFilter::Project),
            "user" => Ok(TemplateSourceFilter::User),
            "builtin" | "built-in" => Ok(TemplateSourceFilter::Builtin),
            "all" => Ok(TemplateSourceFilter::All),
            other => Err(miette!(
                help = "pass --source project, --source user, --source builtin, or \
                        --source all.",
                "invalid template source '{}'. Expected one of: project, user, builtin, all",
                other
            )),
        }
    }

    fn discover_templates(filter: TemplateSourceFilter) -> MietteResult<Vec<DiscoveredTemplate>> {
        let mut templates = Vec::new();
        let mut seen = HashSet::new();

        for (source, root) in template_search_roots(filter)? {
            if source == TemplateSource::Builtin {
                // Built-ins have no search root: they live in the binary and are
                // extracted only when one is actually instantiated. Listing them
                // reads the embedded manifest instead. §FS-rhei-templates.1
                for name in builtin_template_names() {
                    if seen.contains(&name) {
                        continue;
                    }
                    let Ok(extracted) = materialize_builtin_template(&name) else {
                        continue;
                    };
                    let Ok(manifest) = load_template_manifest(extracted.path()) else {
                        continue;
                    };
                    seen.insert(name.clone());
                    templates.push(DiscoveredTemplate {
                        manifest,
                        // A built-in has no stable on-disk location; the name is
                        // how it is referenced.
                        path: PathBuf::from(&name),
                        source,
                    });
                }
                continue;
            }
            if !root.path().is_dir() {
                continue;
            }

            let dir = root.path();
            let mut entries = fs::read_dir(dir)
                .map_err(|err| file_io_report(dir, "failed to read template directory", err))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| {
                    miette!(
                        help = "check that the templates directory is readable, then re-run: rhei templates",
                        "failed to read dir entry in '{}': {err}", dir.display()
                    )
                })?;
            entries.sort_by_key(|entry| entry.file_name());

            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || !path.is_dir() || seen.contains(&name) {
                    continue;
                }

                let Ok(manifest) = load_template_manifest(&path) else {
                    continue;
                };

                // The listing is what was actually read, so a shadowed
                // deprecated root contributes nothing and says nothing.
                // §FS-rhei-templates.1.3
                root.warn_if_deprecated();
                seen.insert(name);
                templates.push(DiscoveredTemplate { manifest, path, source });
            }
        }

        Ok(templates)
    }

    /// Every tier's search roots in priority order. A project or user tier
    /// contributes both home names, the current one first; the deprecated one
    /// is only ever *read*, and reading it warns. §FS-rhei-templates.1
    fn template_search_roots(
        filter: TemplateSourceFilter,
    ) -> MietteResult<Vec<(TemplateSource, RheiHomePath)>> {
        let mut roots = Vec::new();
        let user_home = match filter {
            TemplateSourceFilter::User | TemplateSourceFilter::All => Some(home_dir()?),
            TemplateSourceFilter::Project => home_dir().ok(),
            TemplateSourceFilter::Builtin => None,
        };

        if filter.includes(TemplateSource::Project) {
            roots.extend(
                project_template_roots(user_home.as_deref())?
                    .into_iter()
                    .map(|root| (TemplateSource::Project, root)),
            );
        }
        if filter.includes(TemplateSource::User) {
            roots.extend(
                rhei_home_paths(user_home.as_deref().expect("user filter resolved HOME"), "templates")
                    .into_iter()
                    .map(|root| (TemplateSource::User, root)),
            );
        }
        if filter.includes(TemplateSource::Builtin) {
            // Placeholder path: built-ins are embedded, so this root is never
            // read. It exists so the tier keeps its place in the search order
            // and can be named in the "searched" listing.
            roots.push((
                TemplateSource::Builtin,
                RheiHomePath::plain("<compiled into the rhei binary>"),
            ));
        }

        Ok(roots)
    }

    /// Genuine project template homes without marker or user-home cutoffs;
    /// project-root fallback supplies diagnostics. §FS-rhei-templates.1.2
    fn project_template_roots(user_home: Option<&Path>) -> MietteResult<Vec<RheiHomePath>> {
        let cwd = std::env::current_dir()
            .map_err(|e| miette!(
                help = cwd_help(),
                "failed to determine working directory: {e}"
            ))?;
        Ok(ancestor_template_roots(&cwd, &find_project_root()?, user_home))
    }

    /// A template resolved to a directory the instantiation pipeline can read.
    ///
    /// A built-in is extracted from the binary into a temporary directory; the
    /// handle is carried here so the extraction lives exactly as long as the
    /// resolved template is in use.
    struct ResolvedTemplate {
        path: PathBuf,
        _extracted: Option<ExtractedTemplate>,
    }

    impl ResolvedTemplate {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    fn resolve_template_reference(reference: &str) -> MietteResult<ResolvedTemplate> {
        if template_reference_is_path(reference) {
            let path = PathBuf::from(reference);
            if !path.is_dir() {
                return Err(miette!(
                    help = "a template reference containing '/' is treated as a path. Point it at \
                            the directory holding template.yaml, or drop the '/' to use a \
                            template by name: rhei templates",
                    "template directory '{}' does not exist",
                    path.display()
                ));
            }
            return Ok(ResolvedTemplate { path, _extracted: None });
        }

        for (source, root) in template_search_roots(TemplateSourceFilter::All)? {
            if source == TemplateSource::Builtin {
                // Lowest priority: a project or user template of the same name
                // has already won by the time the search reaches here.
                if builtin_template_exists(reference) {
                    let extracted = materialize_builtin_template(reference)?;
                    return Ok(ResolvedTemplate {
                        path: extracted.path().to_path_buf(),
                        _extracted: Some(extracted),
                    });
                }
                continue;
            }
            let candidate = root.path().join(reference);
            if candidate.is_dir() {
                // Resolved from the deprecated home, so say so. §FS-rhei-templates.1.3
                root.warn_if_deprecated();
                return Ok(ResolvedTemplate { path: candidate, _extracted: None });
            }
        }

        let names = discover_templates(TemplateSourceFilter::All)?
            .into_iter()
            .map(|template| template.manifest.name)
            .collect::<Vec<_>>();

        // §FS-rhei-templates.6.1.2: named-template lookup reports a close discovered match.
        // §FS-rhei-errors.1.3: and when nothing is close, it says how to see the list.
        let hint = nearest_match(reference, &names)
            .map(|name| format!("Did you mean '{name}'? "))
            .unwrap_or_default();
        Err(miette!(
            help = format!("{hint}List the templates you can instantiate with: rhei templates"),
            "template '{}' not found among project, user, or built-in templates",
            reference
        ))
    }

    fn template_reference_is_path(reference: &str) -> bool {
        let path = Path::new(reference);
        path.is_absolute() || reference.contains('/') || reference.starts_with('.')
    }

    fn load_template_manifest(template_dir: &Path) -> MietteResult<TemplateManifest> {
        let manifest_path = template_dir.join("template.yaml");
        let raw = fs::read_to_string(&manifest_path).map_err(|err| {
            file_io_report(&manifest_path, "failed to read template manifest", err)
        })?;
        let mut manifest: TemplateManifest = serde_yaml::from_str(&raw)
            .map_err(|err| miette!(
                help = template_manifest_help(),
                "failed to parse '{}': {err}", manifest_path.display()
            ))?;
        if manifest.select.is_some() {
            let source: YamlValue = serde_yaml::from_str(&raw).map_err(|e| miette!("{e}"))?;
            manifest.static_declarations = ["ports", "data", "compatibility"].into_iter()
                .filter(|key| source.get(*key).is_some()).map(str::to_string).collect();
        }
        validate_template_manifest(&manifest, template_dir)?;
        Ok(manifest)
    }

    fn validate_template_manifest(
        manifest: &TemplateManifest,
        template_dir: &Path,
    ) -> MietteResult<()> {
        let dir_name =
            template_dir.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
                miette!(
                    help = template_manifest_help(),
                    "template path '{}' has no directory name", template_dir.display()
                )
            })?;
        let ident = Regex::new(r"^[A-Za-z][A-Za-z0-9_-]*$")
            .expect("template identifier regex should be valid");

        if manifest.name != dir_name {
            return Err(miette!(
                help = template_manifest_help(),
                "template manifest name '{}' does not match directory '{}'",
                manifest.name,
                dir_name
            ));
        }
        if !ident.is_match(&manifest.name) {
            return Err(miette!(
                help = template_manifest_help(),
                "template name '{}' is not a valid identifier", manifest.name
            ));
        }
        if manifest.description.trim().is_empty() {
            return Err(miette!(
                help = template_manifest_help(),
                "template '{}' must include a non-empty description",
                manifest.name
            ));
        }

        let cwd = std::env::current_dir()
            .map_err(|err| miette!(
                help = cwd_help(),
                "failed to determine working directory: {err}"
            ))?;
        let mut seen = HashSet::new();
        let mut positional_indexes = Vec::new();

        for input in &manifest.inputs {
            if !ident.is_match(&input.name) {
                return Err(miette!(
                    help = template_manifest_help(),
                    "template '{}' input '{}' is not a valid identifier",
                    manifest.name,
                    input.name
                ));
            }
            if !seen.insert(input.name.as_str()) {
                return Err(miette!(
                    help = template_manifest_help(),
                    "template '{}' declares duplicate input '{}'",
                    manifest.name,
                    input.name
                ));
            }
            if input.description.trim().is_empty() {
                return Err(miette!(
                    help = template_manifest_help(),
                    "template '{}' input '{}' must include a description",
                    manifest.name,
                    input.name
                ));
            }
            if let Some(index) = input.positional {
                if index == 0 {
                    return Err(miette!(
                        help = template_manifest_help(),
                        "template '{}' input '{}' positional index must be >= 1",
                        manifest.name,
                        input.name
                    ));
                }
                positional_indexes.push((index, input.name.as_str()));
            }
            if input.schema.required == Some(true) && input.schema.default.is_some() {
                return Err(miette!(
                    help = template_manifest_help(),
                    "template '{}' input '{}' cannot set both required: true and default",
                    manifest.name,
                    input.name
                ));
            }
            validate_template_value_schema(&manifest.name, &input.name, &input.schema)?;
            if let Some(default) = input.schema.default.as_ref() {
                let _ = coerce_template_input_value(input, default, &cwd, true)?;
            }
        }

        positional_indexes.sort_by_key(|(index, _)| *index);
        for (expected, (actual, name)) in positional_indexes.iter().enumerate() {
            let expected = expected + 1;
            if *actual != expected {
                return Err(miette!(
                    help = template_manifest_help(),
                    "template '{}' input '{}' declares positional {}, but positional indexes must be unique and contiguous starting at 1",
                    manifest.name,
                    name,
                    actual
                ));
            }
        }

        match detect_template_layout(template_dir) {
            Ok(_) => {}
            Err(_) if !manifest.block.mounts.is_empty()
                && !template_dir.join("plan.rhei.md").is_file()
                && !template_dir.join("index.rhei.md").is_file() => {}
            Err(err) => return Err(err),
        }

        validate_block_manifest(manifest, template_dir, &ident)?;

        Ok(())
    }

    /// Validate manifest-local block syntax before resolution or rendering.
    /// Cross-block references are checked after recursive expansion.
    /// §FS-rhei-library.1–2 §FS-rhei-library.8
    fn validate_block_manifest(
        manifest: &TemplateManifest,
        template_dir: &Path,
        ident: &Regex,
    ) -> MietteResult<()> {
        let source = template_dir.join("template.yaml");
        let block = &manifest.block;
        if !block.is_block() && manifest.select.is_none() {
            return Ok(());
        }
        if block.ports.is_none() && manifest.select.is_none() {
            return Err(miette!(help = "declare ports.entry and the public exits", "block manifest '{}' must declare ports", source.display()));
        }
        if let Some(ports) = &block.ports {
            if ports.exits.is_empty() {
                return Err(miette!("block manifest '{}' must declare at least one exit port", source.display()));
            }
            for name in ports.exits.keys() {
                if !ident.is_match(name) {
                    return Err(miette!("block manifest '{}' contains invalid public identifier '{name}'; use a letter then letters, digits, '_' or '-'", source.display()));
                }
            }
        }
        for name in block.data.inputs.keys().chain(block.data.outputs.keys()) {
            if !ident.is_match(name) {
                return Err(miette!("block manifest '{}' contains invalid public identifier '{name}'; use a letter then letters, digits, '_' or '-'", source.display()));
            }
        }
        let mut aliases = BTreeMap::<&str, &str>::new();
        for mount in &block.mounts {
            if !ident.is_match(&mount.alias) {
                return Err(miette!(
                    help = "a mount alias must start with a letter and continue with letters, digits, '_' or '-'.",
                    "invalid mount alias '{}' in '{}'",
                    mount.alias,
                    source.display()
                ));
            }
            if let Some(previous) = aliases.insert(&mount.alias, &mount.block) {
                return Err(miette!(
                    "duplicate mount alias '{}' in '{}': '{}' and '{}'",
                    mount.alias,
                    source.display(),
                    previous,
                    mount.block
                ));
            }
        }
        if let Some((first, second, target)) = block.compatibility.collision() {
            return Err(miette!(
                "compatibility collision in '{}': stable identities '{}' and '{}' both target '{}'",
                source.display(), first, second, target
            ));
        }
        Ok(())
    }

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

    fn detect_template_layout(template_dir: &Path) -> MietteResult<TemplateLayout> {
        let plan_path = template_dir.join("plan.rhei.md");
        let index_path = template_dir.join("index.rhei.md");
        let has_plan = plan_path.is_file();
        let has_index = index_path.is_file();

        match (has_plan, has_index) {
            (true, false) => Ok(TemplateLayout::SingleFile),
            (false, true) => {
                let tasks_dir = template_dir.join("tasks");
                if !tasks_dir.is_dir() {
                    return Err(miette!(
                        help = template_manifest_help(),
                        "template '{}' is a workspace template but is missing tasks/",
                        template_dir.display()
                    ));
                }
                Ok(TemplateLayout::Workspace)
            }
            (true, true) => Err(miette!(
                help = template_manifest_help(),
                "template '{}' contains both plan.rhei.md and index.rhei.md",
                template_dir.display()
            )),
            (false, false) => Err(miette!(
                help = template_manifest_help(),
                "template '{}' must contain either plan.rhei.md or index.rhei.md",
                template_dir.display()
            )),
        }
    }

    fn collect_template_inputs(
        manifest: &TemplateManifest,
        template_ref: &str,
        values_files: &[PathBuf],
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
    ) -> MietteResult<BTreeMap<String, serde_json::Value>> {
        let cwd = std::env::current_dir()
            .map_err(|err| miette!(
                help = cwd_help(),
                "failed to determine working directory: {err}"
            ))?;
        let mut raw_values: BTreeMap<String, YamlValue> = BTreeMap::new();

        for values_file in values_files {
            let loaded = load_template_values_file(values_file)?;
            for (key, value) in loaded {
                raw_values.insert(key, value);
            }
        }

        let parsed_input_args = parse_template_input_args(manifest, input_args)?;
        for (key, value) in parsed_input_args.positional_values {
            raw_values.insert(key, YamlValue::String(value));
        }
        for (key, value) in parsed_input_args.assignments {
            raw_values.insert(key, YamlValue::String(value));
        }

        for assignment in set_values {
            let (key, value) = parse_assignment(assignment, "--set")?;
            raw_values.insert(key, YamlValue::String(value));
        }

        for assignment in set_files {
            let (key, value_path) = parse_assignment(assignment, "--set-file")?;
            let path = PathBuf::from(value_path);
            let contents = fs::read_to_string(&path)
                .map_err(|err| file_io_report(&path, "failed to read --set-file input", err))?;
            raw_values.insert(key, YamlValue::String(contents));
        }

        let declared_inputs =
            manifest.inputs.iter().map(|input| input.name.clone()).collect::<Vec<_>>();
        for key in raw_values.keys() {
            if !declared_inputs.iter().any(|name| name == key) {
                // §FS-rhei-errors.1.3: name the near miss instead of leaving the
                // user to diff their spelling against `--list-inputs` output.
                return Err(miette!(
                    help = format!(
                        "{}List every input with: {}",
                        did_you_mean(key, &declared_inputs)
                            .map(|hint| format!("{hint} "))
                            .unwrap_or_default(),
                        list_inputs_command(template_ref)
                    ),
                    "template '{}' has no input named '{}'",
                    manifest.name,
                    key
                ));
            }
        }

        // Report every missing input at once: one at a time turns supplying
        // inputs into a guessing loop where each attempt buys exactly one more
        // field name. §FS-rhei-templates.6.1.1 §FS-rhei-errors.1.1
        let missing = manifest
            .inputs
            .iter()
            .filter(|input| {
                input.is_required()
                    && input.schema.default.is_none()
                    && !raw_values.contains_key(&input.name)
            })
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(missing_template_inputs_report(
                manifest,
                template_ref,
                &missing,
                values_files,
                input_args,
                set_values,
                set_files,
            ));
        }

        // §FS-rhei-errors.1.1: a rejected value is the same failure class as a
        // missing one, so a three-input mistake costs one round trip, not three.
        let mut resolved = BTreeMap::new();
        let mut rejected = Vec::new();
        for input in &manifest.inputs {
            let value = if let Some(raw) = raw_values.get(&input.name) {
                coerce_template_input_value(input, raw, &cwd, false)
            } else if let Some(default) = input.schema.default.as_ref() {
                coerce_template_input_value(input, default, &cwd, true)
            } else {
                Ok(empty_template_value(&input.schema))
            };

            let value = match value {
                Ok(value) => value,
                Err(report) => {
                    rejected.push((input.name.clone(), report));
                    continue;
                }
            };

            if let Err(report) = validate_resolved_value(&input.name, &input.schema, &value) {
                rejected.push((input.name.clone(), report));
                continue;
            }

            resolved.insert(input.name.clone(), value);
        }
        if !rejected.is_empty() {
            return Err(rejected_template_inputs_report(rejected, template_ref));
        }

        Ok(resolved)
    }

    /// Fold the per-input rejections into one diagnostic, keeping each input's
    /// own message and its own remedy. §FS-rhei-errors.1.1
    fn rejected_template_inputs_report(
        rejected: Vec<(String, Report)>,
        template_ref: &str,
    ) -> Report {
        // One bad input is already a complete diagnostic; wrapping it would only
        // bury its remedy under a summary line.
        if rejected.len() == 1 {
            let (_, report) = rejected.into_iter().next().expect("exactly one rejection");
            return with_list_inputs_pointer(&report, template_ref);
        }

        let listed = rejected
            .iter()
            .map(|(_, report)| format!("  {report}"))
            .collect::<Vec<_>>()
            .join("\n");
        // Remedies go in the help block, not the message: miette re-indents a
        // wrapped help line and does not re-indent a wrapped message line, so a
        // remedy nested into the message loses its nesting on a narrow terminal.
        // Each is prefixed with the input it repairs, which is what pairs it
        // with its failure now that the two are no longer adjacent.
        let remedies = rejected
            .iter()
            .filter_map(|(name, report)| {
                report.help().map(|help| {
                    let text = help.to_string();
                    let joined =
                        text.lines().map(str::trim).collect::<Vec<_>>().join(" ");
                    format!("{name}: {joined}")
                })
            })
            .collect::<Vec<_>>()
            .join("\n");
        miette!(
            help = format!(
                "{remedies}\nSee what every input accepts: {}",
                list_inputs_command(template_ref)
            ),
            "{} inputs were rejected:\n{listed}",
            rejected.len()
        )
    }

    /// The same report with `--list-inputs` appended to its help. Added here,
    /// not by each check, so a batch prints it once. §FS-rhei-errors.1.2
    fn with_list_inputs_pointer(report: &Report, template_ref: &str) -> Report {
        let pointer =
            format!("See what every input accepts: {}", list_inputs_command(template_ref));
        let help = match report.help() {
            Some(help) => format!("{help}\n{pointer}"),
            None => pointer,
        };
        miette!(help = help, "{report}")
    }

    /// Check a resolved scalar against the named `format` its input declares,
    /// so the failure names the input the user typed. §FS-rhei-errors.3.1
    fn validate_template_input_format(
        label: &str,
        format: TemplateInputFormat,
        rendered: &str,
    ) -> MietteResult<()> {
        match format {
            TemplateInputFormat::ExecutionTarget => {
                rhei_validator::parse_execution_target(rendered).map_err(|err| {
                    // §FS-rhei-errors.1.2: the example is keyed to what the
                    // user can actually type, not to the label.
                    miette!(
                        help = format!(
                            "{err}.\n{}",
                            execution_target_repair_example(label, rendered)
                        ),
                        "input '{}' is not a valid execution target: '{}'",
                        label,
                        rendered
                    )
                })?;
                Ok(())
            }
        }
    }

    /// A corrected value for `label`, written the way the user would supply it:
    /// an assignment for a top-level input, and for a nested scalar the value
    /// alone, since `reviewers[0]=…` is not CLI syntax. §FS-rhei-errors.1.2
    fn execution_target_repair_example(label: &str, rendered: &str) -> String {
        let example = rhei_validator::execution_target_example(rendered);
        let nested = label.contains('[') || label.contains('.');
        if !nested {
            return format!("A corrected value for this input: {}", shell_assignment(label, &example));
        }
        let root = label.split(['[', '.']).next().unwrap_or(label);
        format!(
            "A corrected value for '{label}': {}. Supply it inside the whole '{root}' value, \
             which is written as one YAML or JSON literal.",
            shell_quote(&example)
        )
    }

    /// The command that lists a template's inputs, quoted for paste.
    fn list_inputs_command(template_ref: &str) -> String {
        shell_command(["rhei", "instantiate", template_ref, "--list-inputs"])
    }

    /// Build the "you are missing these inputs" diagnostic: every missing name
    /// with its description, and a runnable command that carries the arguments
    /// already supplied. §FS-rhei-errors.1
    #[allow(clippy::too_many_arguments)]
    fn missing_template_inputs_report(
        manifest: &TemplateManifest,
        template_ref: &str,
        missing: &[&TemplateInputDef],
        values_files: &[PathBuf],
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
    ) -> Report {
        let listed = missing
            .iter()
            .map(|input| {
                format!("  {} ({}) — {}", input.name, input.value_type().as_str(), input.description)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let placeholders = missing
            .iter()
            .map(|input| format!("{}={}", input.name, template_input_placeholder(input)))
            .collect::<Vec<_>>();
        let command = format_template_instantiation_command(
            template_ref,
            input_args,
            set_values,
            set_files,
            values_files,
            None,
            &placeholders,
        );

        // Name the input most likely to hold prose, not merely the first string
        // one: suggesting `--set-file subject=<path>` for a one-word title while
        // the essay-length brief sits below it reads as noise.
        let long_value_hint = missing
            .iter()
            .filter(|input| matches!(input.value_type(), TemplateInputType::String))
            .max_by_key(|input| input.description.chars().count())
            .map(|input| {
                format!(
                    "\nFor a long value, read it from a file: --set-file {}=<path>",
                    input.name
                )
            })
            .unwrap_or_default();

        let noun = if missing.len() == 1 { "input" } else { "inputs" };
        miette!(
            help = format!(
                "replace each <…> and run:\n  {command}{long_value_hint}\nList every input \
                 with: {}",
                list_inputs_command(template_ref)
            ),
            "template '{}' is missing {} required {noun}:\n{listed}",
            manifest.name,
            missing.len()
        )
    }

    /// A placeholder that shows the shape of a value rather than the word
    /// "value" — arrays and objects are supplied as YAML/JSON snippets, and a
    /// user who is told `<value>` will guess wrong.
    fn template_input_placeholder(input: &TemplateInputDef) -> String {
        match input.value_type() {
            TemplateInputType::Number => "<number>".to_string(),
            TemplateInputType::Boolean => "<true|false>".to_string(),
            TemplateInputType::Path => "<path>".to_string(),
            TemplateInputType::Array => "<[item, item]>".to_string(),
            TemplateInputType::Object => "<{key: value}>".to_string(),
            TemplateInputType::String => "<value>".to_string(),
        }
    }

    /// Enforce each scalar `validate` pattern in `schema` against the matching
    /// scalar in the already-coerced `value`, recursing through array items and
    /// object properties. This is what makes `validate` declared on a nested
    /// `properties.<x>` or array `items` scalar take effect — not only on
    /// top-level inputs. Patterns are guaranteed valid here because
    /// `validate_template_value_schema` compiled them at manifest-load time.
    fn validate_resolved_value(
        label: &str,
        schema: &TemplateValueSchema,
        value: &serde_json::Value,
    ) -> MietteResult<()> {
        if let Some(pattern) = schema.validate.as_deref() {
            let regex = compile_full_match_regex(pattern).map_err(|err| {
                miette!(
                    help = internal_error_help(),
                    "input '{}' has invalid validate regex: {err}",
                    label
                )
            })?;
            let rendered = scalar_template_value_as_string(value).ok_or_else(|| {
                miette!(
                    help = internal_error_help(),
                    "input '{}' uses validate but did not resolve to a scalar string value",
                    label
                )
            })?;
            if !regex.is_match(&rendered) {
                return Err(miette!(
                    help = format!("'{rendered}' is rejected by the input's own pattern."),
                    "input '{}' does not match validation pattern '{}'",
                    label,
                    pattern
                ));
            }
        }

        // §FS-rhei-errors.3.1: a named format fails here, against the input the
        // user typed, instead of later against a rendered file they never wrote.
        if let Some(format) = schema.format {
            let rendered = scalar_template_value_as_string(value).ok_or_else(|| {
                miette!(
                    help = internal_error_help(),
                    "input '{}' uses format but did not resolve to a scalar value",
                    label
                )
            })?;
            validate_template_input_format(label, format, &rendered)?;
        }

        match schema.value_type {
            TemplateInputType::Array => {
                if let (Some(items), serde_json::Value::Array(elements)) =
                    (schema.items.as_deref(), value)
                {
                    for (idx, element) in elements.iter().enumerate() {
                        validate_resolved_value(
                            &format!("{label}[{idx}]"),
                            items,
                            element,
                        )?;
                    }
                }
            }
            TemplateInputType::Object => {
                if let serde_json::Value::Object(map) = value {
                    for (property, property_schema) in &schema.properties {
                        if let Some(element) = map.get(property) {
                            validate_resolved_value(
                                &format!("{label}.{property}"),
                                property_schema,
                                element,
                            )?;
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
