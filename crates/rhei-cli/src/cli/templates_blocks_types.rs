    // Typed CLI-side ownership and compiler records bridge manifest values to
    // one ordinary workspace. §AR-rhei-library.1–3
    use rhei_core::blocks::{split_endpoint, Block, BlockManifest, CompiledBlock, CompiledFile, Fragment, Mount, Seam, TaskFile};

    /// Values already reduced through the direct-composition precedence
    /// chain, keyed by `<mount>.<input>`. §FS-rhei-library.3
    #[derive(Default)]
    struct MountedInputValues {
        values: BTreeMap<String, YamlValue>,
    }

    impl MountedInputValues {
        fn from_cli(
            values_files: &[PathBuf],
            set_values: &[String],
            set_files: &[String],
        ) -> MietteResult<Self> {
            let mut values = BTreeMap::new();
            for path in values_files {
                for (alias, nested) in load_template_values_file(path)? {
                    let YamlValue::Mapping(nested) = nested else {
                        return Err(miette!(
                            help = "use an alias mapping, for example `review: { target: HEAD }`.",
                            "values file '{}' entry '{}' must be a mapping of mounted inputs",
                            path.display(), alias
                        ));
                    };
                    for (input, value) in nested {
                        let Some(input) = input.as_str() else {
                            return Err(miette!("values for mount '{}' contain a non-string input name", alias));
                        };
                        values.insert(format!("{alias}.{input}"), value);
                    }
                }
            }
            for assignment in set_values {
                let (key, value) = parse_assignment(assignment, "--set")?;
                values.insert(key, YamlValue::String(value));
            }
            for assignment in set_files {
                let (key, path) = parse_assignment(assignment, "--set-file")?;
                let path = PathBuf::from(path);
                let contents = fs::read_to_string(&path)
                    .map_err(|err| file_io_report(&path, "failed to read --set-file input", err))?;
                values.insert(key, YamlValue::String(contents));
            }
            Ok(Self { values })
        }

        fn for_alias(&self, alias: &str) -> BTreeMap<String, YamlValue> {
            let prefix = format!("{alias}.");
            self.values
                .iter()
                .filter_map(|(key, value)| {
                    key.strip_prefix(&prefix).map(|key| (key.to_string(), value.clone()))
                })
                .collect()
        }

        fn validate_aliases(&self, aliases: &[String]) -> MietteResult<()> {
            for key in self.values.keys() {
                let Some((alias, _)) = key.split_once('.') else {
                    return Err(miette!(
                        help = "mounted inputs use `<alias>.<input>`, for example `review.target`.",
                        "mounted input '{}' is not qualified by an alias",
                        key
                    ));
                };
                if !aliases.iter().any(|known| known == alias) {
                    return Err(miette!(
                        help = format!("available mount aliases: {}", aliases.join(", ")),
                        "mounted input '{}' names unknown alias '{}'",
                        key, alias
                    ));
                }
            }
            Ok(())
        }
    }
