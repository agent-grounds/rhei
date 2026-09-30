    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateSource {
        Project,
        User,
        /// Shipped inside the binary; the tier every install has. §FS-rhei-templates.1
        Builtin,
    }

    impl TemplateSource {
        fn as_str(self) -> &'static str {
            match self {
                TemplateSource::Project => "project",
                TemplateSource::User => "user",
                TemplateSource::Builtin => "built-in",
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateSourceFilter {
        Project,
        User,
        Builtin,
        All,
    }

    impl TemplateSourceFilter {
        fn includes(self, source: TemplateSource) -> bool {
            matches!(
                (self, source),
                (TemplateSourceFilter::All, _)
                    | (TemplateSourceFilter::Project, TemplateSource::Project)
                    | (TemplateSourceFilter::User, TemplateSource::User)
                    | (TemplateSourceFilter::Builtin, TemplateSource::Builtin)
            )
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateLayout {
        SingleFile,
        Workspace,
    }

    impl TemplateLayout {
        fn entrypoint(self, output_dir: &Path) -> PathBuf {
            match self {
                TemplateLayout::SingleFile => output_dir.join("plan.rhei.md"),
                TemplateLayout::Workspace => output_dir.to_path_buf(),
            }
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct TemplateManifest {
        name: String,
        version: YamlValue,
        description: String,
        #[serde(default)]
        inputs: Vec<TemplateInputDef>,
        /// Templates this one is built out of, unioned in list order.
        /// §FS-rhei-library.6
        #[serde(default)]
        includes: Vec<TemplateInclude>,
    }

    impl TemplateManifest {
        fn version_string(&self) -> String {
            format_version(&self.version)
        }

        fn required_input_count(&self) -> usize {
            self.inputs.iter().filter(|input| input.is_required()).count()
        }

        fn inputs_summary(&self) -> String {
            if self.inputs.is_empty() {
                return "none".to_string();
            }

            self.inputs
                .iter()
                .map(|input| {
                    if input.is_required() {
                        input.name.clone()
                    } else {
                        format!("{}?", input.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct TemplateInputDef {
        name: String,
        description: String,
        #[serde(default)]
        positional: Option<usize>,
        #[serde(flatten)]
        schema: TemplateValueSchema,
    }

    impl TemplateInputDef {
        fn is_required(&self) -> bool {
            self.schema.is_required()
        }

        fn value_type(&self) -> TemplateInputType {
            self.schema.value_type
        }
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct TemplateValueSchema {
        #[serde(default, rename = "type")]
        value_type: TemplateInputType,
        #[serde(default)]
        required: Option<bool>,
        #[serde(default)]
        default: Option<YamlValue>,
        #[serde(default)]
        validate: Option<String>,
        /// Built-in value check applied at instantiation time.
        /// §FS-rhei-templates.3.1
        #[serde(default)]
        format: Option<TemplateInputFormat>,
        #[serde(default)]
        items: Option<Box<TemplateValueSchema>>,
        #[serde(default)]
        properties: BTreeMap<String, TemplateValueSchema>,
    }

    impl TemplateValueSchema {
        fn is_required(&self) -> bool {
            self.required.unwrap_or(self.default.is_none())
        }
    }

    /// A named value check a template input can declare, applied where the
    /// user typed the value, not in the rendered file. §FS-rhei-errors.3.1
    #[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    enum TemplateInputFormat {
        ExecutionTarget,
    }

    impl TemplateInputFormat {
        fn as_str(self) -> &'static str {
            match self {
                TemplateInputFormat::ExecutionTarget => "execution-target",
            }
        }
    }

    #[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum TemplateInputType {
        #[default]
        String,
        Number,
        Boolean,
        Path,
        Array,
        Object,
    }

    impl TemplateInputType {
        fn as_str(self) -> &'static str {
            match self {
                TemplateInputType::String => "string",
                TemplateInputType::Number => "number",
                TemplateInputType::Boolean => "boolean",
                TemplateInputType::Path => "path",
                TemplateInputType::Array => "array",
                TemplateInputType::Object => "object",
            }
        }
    }

    #[derive(Debug, Clone)]
    struct DiscoveredTemplate {
        manifest: TemplateManifest,
        path: PathBuf,
        source: TemplateSource,
    }

    #[derive(Debug)]
    struct MaterializedTemplate {
        layout: TemplateLayout,
        output_dir: PathBuf,
    }

    impl MaterializedTemplate {
        fn entrypoint(&self) -> PathBuf {
            self.layout.entrypoint(&self.output_dir)
        }

        fn state_machine_path(&self) -> Option<PathBuf> {
            let path = self.output_dir.join("states.yaml");
            path.is_file().then_some(path)
        }
    }

    pub(super) fn templates_command(
        as_json: bool,
        source_filter: &str,
        template: Option<&str>,
    ) -> MietteResult<()> {
        let filter = parse_template_source_filter(source_filter)?;
        let templates = discover_templates(filter)?;

        // Naming a template after reading the list answers with its detail;
        // resolution searches every tier — the filter shapes the list only.
        // §FS-rhei-templates.6.3
        if let Some(reference) = template {
            let all = if filter == TemplateSourceFilter::All {
                templates
            } else {
                discover_templates(TemplateSourceFilter::All)?
            };
            return template_detail(as_json, &all, reference);
        }

        if as_json {
            let payload = templates.iter().map(template_json_entry).collect::<Vec<_>>();
            let rendered = serde_json::to_string_pretty(&payload)
                .map_err(|err| miette!(
                    help = internal_error_help(),
                    "failed to serialize template listing: {err}"
                ))?;
            println!("{rendered}");
            return Ok(());
        }

        if templates.is_empty() {
            println!("No templates found.");
            let roots = template_search_roots(filter)?;
            if !roots.is_empty() {
                println!("Searched:");
                for (source, root) in roots {
                    let root = root.path();
                    let exists_marker = if root.is_dir() { "" } else { " (does not exist)" };
                    println!("  [{}] {}{}", source.as_str(), root.display(), exists_marker);
                }
            }
            return Ok(());
        }

        println!("Templates:");
        for template in templates {
            println!(
                "{}  {}  {}",
                template.manifest.name,
                template.manifest.version_string(),
                template.source.as_str(),
            );
            println!("  {}", template.manifest.description);
            println!("  inputs: {}", template.manifest.inputs_summary());
        }

        Ok(())
    }

    /// One list entry in the JSON shape shared by the listing and the detail
    /// view. §FS-rhei-templates.6.3
    fn template_json_entry(template: &DiscoveredTemplate) -> serde_json::Value {
        serde_json::json!({
            "name": template.manifest.name,
            "version": template.manifest.version_string(),
            "description": template.manifest.description,
            "source": template.source.as_str(),
            "path": template.path,
            "required_inputs": template.manifest.required_input_count(),
            "inputs": template.manifest.inputs.iter().map(|input| {
                let mut entry = template_schema_json(&input.schema);
                entry["name"] = serde_json::Value::String(input.name.clone());
                entry["description"] = serde_json::Value::String(input.description.clone());
                entry["positional"] = match input.positional {
                    Some(slot) => serde_json::Value::from(slot),
                    None => serde_json::Value::Null,
                };
                entry
            }).collect::<Vec<_>>(),
        })
    }

    /// One value schema as JSON, nested schemas and all: every key the
    /// manifest declared, so a caller can build an input form from the
    /// listing alone. §FS-rhei-templates.6.3.1
    fn template_schema_json(schema: &TemplateValueSchema) -> serde_json::Value {
        serde_json::json!({
            "type": schema.value_type.as_str(),
            "required": schema.is_required(),
            "default": schema.default.as_ref(),
            "validate": schema.validate.as_ref(),
            "format": schema.format.map(TemplateInputFormat::as_str),
            "items": schema.items.as_ref().map(|items| template_schema_json(items)),
            "properties": match schema.properties.is_empty() {
                true => serde_json::Value::Null,
                false => schema
                    .properties
                    .iter()
                    .map(|(name, property)| (name.clone(), template_schema_json(property)))
                    .collect::<serde_json::Map<_, _>>()
                    .into(),
            },
        })
    }

    /// One template in detail: the list row expanded to the full input schema,
    /// where the template comes from, and how to instantiate it.
    /// §FS-rhei-templates.6.3
    fn template_detail(
        as_json: bool,
        templates: &[DiscoveredTemplate],
        reference: &str,
    ) -> MietteResult<()> {
        if let Some(template) = templates.iter().find(|t| t.manifest.name == reference) {
            if as_json {
                let rendered = serde_json::to_string_pretty(&template_json_entry(template))
                    .map_err(|err| miette!(
help = internal_error_help(),
"failed to serialize template detail: {err}"))?;
                println!("{rendered}");
                return Ok(());
            }
            println!("Source: {}", template.source.as_str());
            if template.source != TemplateSource::Builtin {
                println!("Path: {}", template.path.display());
            }
            print_template_inputs(&template.manifest, reference);
            return Ok(());
        }

        // A path reference still resolves; the resolver also owns the
        // did-you-mean error for real misses. §FS-rhei-templates.6.1.2
        let resolved = resolve_template_reference(reference)?;
        let manifest = load_template_manifest(resolved.path())?;
        if as_json {
            let entry = DiscoveredTemplate {
                manifest,
                path: resolved.path().to_path_buf(),
                source: TemplateSource::Project,
            };
            let mut value = template_json_entry(&entry);
            // A directory reference has no discovery tier to report.
            value["source"] = serde_json::Value::Null;
            let rendered = serde_json::to_string_pretty(&value)
                .map_err(|err| miette!(
help = internal_error_help(),
"failed to serialize template detail: {err}"))?;
            println!("{rendered}");
            return Ok(());
        }
        println!("Path: {}", resolved.path().display());
        print_template_inputs(&manifest, reference);
        Ok(())
    }
