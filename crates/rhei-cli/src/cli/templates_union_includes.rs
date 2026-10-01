    // `includes:` in `template.yaml`: a template built out of templates, whose
    // parts are rendered with the unioned input values and unioned in list
    // order before anything validates.

    // §FS-rhei-library.6 §FS-rhei-library.6.1 §FS-rhei-library.3.4

    /// One `includes:` entry: a bare name, or a mapping that also says which
    /// task of the host its tickets go under. §FS-rhei-library.6
    #[derive(Debug, Clone, Deserialize)]
    #[serde(untagged)]
    enum TemplateInclude {
        Named(String),
        Entry {
            template: String,
            #[serde(default)]
            under: Option<String>,
        },
    }

    impl TemplateInclude {
        fn template(&self) -> &str {
            match self {
                Self::Named(name) => name,
                Self::Entry { template, .. } => template,
            }
        }

        fn under(&self) -> Option<&str> {
            match self {
                Self::Named(_) => None,
                Self::Entry { under, .. } => under.as_deref(),
            }
        }
    }

    /// Resolve an entry's template: through the same discovery as `<template>`,
    /// or as a path relative to the including template, so a library may keep
    /// private pieces beside the template that composes them.
    ///
    /// The `ResolvedTemplate` is returned rather than its path because a
    /// built-in is extracted to a temporary directory that lives exactly as
    /// long as the handle: dropping it deletes the template out from under the
    /// caller. §FS-rhei-library.6 §FS-rhei-templates.1
    fn resolve_include(including: &Path, name: &str) -> MietteResult<ResolvedTemplate> {
        let beside = including.join(name);
        if beside.join("template.yaml").is_file() {
            return Ok(ResolvedTemplate { path: beside, _extracted: None });
        }
        resolve_template_reference(name)
    }

    /// The union of a template's own inputs and every included template's, so
    /// an including template is used exactly as a flat one is.
    ///
    /// The including template's declaration wins over an included default, and
    /// two included defaults that differ with no declaration above them are an
    /// error naming both templates and the input. §FS-rhei-library.3.4
    fn union_inputs(
        template_dir: &Path,
        manifest: &TemplateManifest,
    ) -> MietteResult<Vec<TemplateInputDef>> {
        let mut inputs = manifest.inputs.clone();
        let mut owners: BTreeMap<String, String> =
            inputs.iter().map(|input| (input.name.clone(), manifest.name.clone())).collect();
        let declared: BTreeSet<String> =
            manifest.inputs.iter().map(|input| input.name.clone()).collect();
        for entry in &manifest.includes {
            let resolved = resolve_include(template_dir, entry.template())?;
            let included_dir = resolved.path();
            let included = load_template_manifest(included_dir)?;
            for input in union_inputs(included_dir, &included)? {
                if declared.contains(&input.name) {
                    continue;
                }
                match inputs.iter().find(|existing| existing.name == input.name) {
                    Some(existing) if same_input(existing, &input) => continue,
                    Some(_) => {
                        let first = owners.get(&input.name).cloned().unwrap_or_default();
                        return Err(miette!(
                            help = "declare the input in the including template to fix its \
                                    value, or make the two declarations agree.",
                            "templates '{first}' and '{}' declare input '{}' differently, and \
                             no declaration above them settles it",
                            included.name,
                            input.name
                        ));
                    }
                    None => {
                        owners.insert(input.name.clone(), included.name.clone());
                        inputs.push(input);
                    }
                }
            }
        }
        Ok(inputs)
    }

    /// Two declarations of one input are one input when they agree on
    /// everything a caller passes. §FS-rhei-library.3.4
    fn same_input(left: &TemplateInputDef, right: &TemplateInputDef) -> bool {
        left.schema == right.schema
    }

    /// Refuse a cycle naming the chain, so the message says which templates
    /// form the loop rather than that recursion ran out. §FS-rhei-library.6
    fn check_include_cycles(
        template_dir: &Path,
        manifest: &TemplateManifest,
        chain: &mut Vec<String>,
    ) -> MietteResult<()> {
        if chain.contains(&manifest.name) {
            chain.push(manifest.name.clone());
            return Err(miette!(
                help = "a template may not include itself, directly or through another: break \
                        the loop by lifting the shared part into a template of its own.",
                "`includes:` cycle: {}",
                chain.join(" -> ")
            ));
        }
        chain.push(manifest.name.clone());
        for entry in &manifest.includes {
            let resolved = resolve_include(template_dir, entry.template())?;
            let included = load_template_manifest(resolved.path())?;
            check_include_cycles(resolved.path(), &included, chain)?;
        }
        chain.pop();
        Ok(())
    }

    /// Render every included template with the unioned values and union them
    /// into `rendered` in list order, then leave the whole to be validated.
    /// §FS-rhei-library.6
    fn apply_includes(
        template_dir: &Path,
        manifest: &TemplateManifest,
        values: &BTreeMap<String, serde_json::Value>,
        rendered: &Path,
        layout: TemplateLayout,
    ) -> MietteResult<()> {
        if manifest.includes.is_empty() {
            return Ok(());
        }
        let index = match layout {
            TemplateLayout::Workspace => rendered.join("index.rhei.md"),
            TemplateLayout::SingleFile => rendered.join("plan.rhei.md"),
        };
        for entry in &manifest.includes {
            let resolved = resolve_include(template_dir, entry.template())?;
            let included_dir = resolved.path();
            let included = load_template_manifest(included_dir)?;
            let scratch = tempfile::tempdir().map_err(|err| {
                miette!(
                    help = "an included template is rendered into a temp directory before it \
                            joins the host. Check that $TMPDIR exists and is writable.",
                    "failed to create the render directory: {err}"
                )
            })?;
            let part_root = scratch.path().join(&included.name);
            let part = render_part(included_dir, &included, values, &part_root, entry.under())?;
            let single_file = layout == TemplateLayout::SingleFile;
            let host = UnionHost {
                root: rendered.to_path_buf(),
                index: index.clone(),
                single_file,
                machine: rendered.join("states.yaml"),
                parent: entry.under().map(str::to_owned),
                project: union_project(rendered, single_file),
            };
            let declaration = resolve_host_machine(&host)?;
            union_into_host(&host, &part, &declaration, UnionMode::Compose).map_err(|err| {
                // The entry is what an author fixes, so it leads the message
                // rather than trailing the refusal as a hint.
                miette!(
                    help = err.help().map_or_else(
                        || "read the refusal above against the entry's `under:`.".to_owned(),
                        |help| help.to_string(),
                    ),
                    "`includes:` entry '{}' of template '{}': {err}",
                    entry.template(),
                    manifest.name
                )
            })?;
        }
        Ok(())
    }

    /// Render one template into `root`, applying its own `includes:` first, so
    /// what joins a host is always a whole rhei. §FS-rhei-library.6
    fn render_part(
        template_dir: &Path,
        manifest: &TemplateManifest,
        values: &BTreeMap<String, serde_json::Value>,
        root: &Path,
        _under: Option<&str>,
    ) -> MietteResult<RenderedPart> {
        let layout = detect_template_layout(template_dir)?;
        materialize_template(template_dir, &manifest.name, layout, root, values, true)?;
        apply_includes(template_dir, manifest, values, root, layout)?;
        let inputs = manifest
            .inputs
            .iter()
            .filter_map(|input| {
                values.get(&input.name).map(|value| (input.name.clone(), value.clone()))
            })
            .collect();
        Ok(RenderedPart {
            name: manifest.name.clone(),
            version: manifest.version_string(),
            digest: template_digest(template_dir)?,
            inputs,
            root: root.to_path_buf(),
        })
    }

    /// A stable digest of a template's source, which is the `src:sha256:` half
    /// of the fence comment. §FS-rhei-library.7.1
    fn template_digest(template_dir: &Path) -> MietteResult<String> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hash_tree(template_dir, template_dir, &mut hasher)?;
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// A relative path's contribution to the digest, spelled the one way on
    /// every platform.
    ///
    /// The digest is the `src:sha256:` half of a fence comment, which a user
    /// reads out of their own `states.yaml`, so it may not depend on which
    /// separator the host spells a nested entry with: `prompt_templates/split.md`
    /// and `prompt_templates\split.md` are one source file and must digest
    /// alike. Joined from `components()` rather than substituted, because on a
    /// POSIX host a backslash is an ordinary character in a file name and
    /// rewriting it would fold two different templates together.
    /// §REQ-cross-platform.2 §FS-rhei-library.7.1
    fn digest_path_component(relative: &Path) -> String {
        relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }

    fn hash_tree(root: &Path, path: &Path, hasher: &mut sha2::Sha256) -> MietteResult<()> {
        use sha2::Digest;
        if path.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(path);
            hasher.update(digest_path_component(relative).as_bytes());
            hasher.update(
                fs::read(path).map_err(|err| file_io_report(path, "failed to read", err))?,
            );
            return Ok(());
        }
        if !path.is_dir() {
            return Ok(());
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(path)
            .map_err(|err| file_io_report(path, "failed to read a template directory", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            hash_tree(root, &entry, hasher)?;
        }
        Ok(())
    }
