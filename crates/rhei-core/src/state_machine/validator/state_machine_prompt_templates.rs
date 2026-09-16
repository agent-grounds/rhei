// §FS-rhei-states.4.4: Reusable prompt templates live beside `states.yaml` as
// Markdown files, are bound to concrete values per state, and compose with the
// inline `instructions` / `personality` a state already declares.

/// Directory name holding the reusable prompt Markdown files for a machine.
const PROMPT_TEMPLATES_DIR: &str = "prompt_templates";

/// Legacy single-file form replaced by the sibling directory of Markdown files.
const LEGACY_PROMPT_TEMPLATES_FILE: &str = "prompt-templates.yaml";

/// Resolve the `prompt_templates/` directory that belongs to a `states.yaml`.
///
/// Every surface that has to find the prompt files — loading, `rhei validate
/// --watch`, tooling — goes through this so they cannot drift apart.
// §FS-rhei-states.4.4: prompt files are a sibling directory of the state machine.
pub fn prompt_templates_dir(state_machine_path: &Path) -> PathBuf {
    state_machine_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(PROMPT_TEMPLATES_DIR)
}

impl StateMachine {
    /// The selected required instruction source, before its bytes are composed.
    /// §FS-rhei-plan-language.3.13
    pub fn prompt_template_source(&self, state: &StateDef) -> Option<&Path> {
        let reference = state.prompt_template.as_ref()?;
        self.prompt_templates.get(reference.name().trim())?.source.as_deref()
    }

    /// Effective agent instructions for a state: reusable template text first,
    /// then the state's own inline text.
    // §FS-rhei-states.4.4: template prompt text is emitted before inline state text.
    pub fn effective_instructions(&self, state: &StateDef) -> Option<String> {
        join_prompt_parts([
            self.prompt_template_instructions(state),
            state.instructions.clone(),
        ])
    }

    /// Effective personality for a state.
    ///
    /// Prompt templates carry instructions only — a `.md` file has no place to
    /// declare personality — so this is the state's own inline framing,
    /// normalized the same way as [`Self::effective_instructions`].
    pub fn effective_personality(&self, state: &StateDef) -> Option<String> {
        join_prompt_parts([state.personality.clone()])
    }

    /// Instructions contributed by the state's selected prompt template, with
    /// `prompt_template.values` already substituted.
    fn prompt_template_instructions(&self, state: &StateDef) -> Option<String> {
        let reference = state.prompt_template.as_ref()?;
        let template = self.prompt_templates.get(reference.name().trim())?;
        Some(substitute_prompt_template_values(&template.instructions, reference))
    }

    /// Validate reusable prompt template declarations and per-state references.
    // §FS-rhei-states.4.4: prompt templates must resolve their concrete placeholders.
    fn validate_prompt_templates(&self) -> Result<(), StateMachineLoadError> {
        for (template_name, template) in &self.prompt_templates {
            if template_name.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "prompt_templates contains an empty template id".to_string(),
                ));
            }
            if template.instructions.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "prompt template '{template_name}' is empty: \
                     '{PROMPT_TEMPLATES_DIR}/{template_name}.md' must contain Markdown prompt text"
                )));
            }
        }

        for (state_name, state) in &self.states {
            self.validate_state_prompt_template(state_name, state)?;
        }

        Ok(())
    }

    pub fn validate_state_prompt_template(
        &self,
        state_name: &str,
        state: &StateDef,
    ) -> Result<(), StateMachineLoadError> {
        let Some(reference) = state.prompt_template.as_ref() else {
            return Ok(());
        };
        let template_name = reference.name().trim();
        if template_name.is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' declares an empty 'prompt_template' name"
            )));
        }
        let template = self
            .prompt_templates
            .get(template_name)
            .ok_or_else(|| self.unknown_prompt_template_error(state_name, template_name))?;

        let values = reference.values();
        if let Some(values) = values {
            for (key, value) in values {
                if !is_prompt_template_placeholder_name(key) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' prompt_template.values contains invalid key '{key}' (expected an identifier)"
                    )));
                }
                if !is_prompt_template_scalar_value(value) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' prompt_template.values.{key} must be a scalar value"
                    )));
                }
            }
        }

        for token in extract_runtime_template_tokens(&template.instructions) {
            if is_prompt_template_control_token(token) || !is_prompt_template_placeholder_name(token)
            {
                continue;
            }
            if values.is_some_and(|values| values.contains_key(token)) {
                continue;
            }
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' prompt template '{template_name}' uses placeholder \
                 '{{{token}}}' but prompt_template.values does not supply '{token}'"
            )));
        }

        Ok(())
    }

    /// Explain an unresolved reference. An empty template map almost always
    /// means the machine was parsed from a string rather than a file, because
    /// prompt files are only discoverable relative to a `states.yaml` path —
    /// naming the id alone would send the reader hunting for a typo that is
    /// not there.
    fn unknown_prompt_template_error(
        &self,
        state_name: &str,
        template_name: &str,
    ) -> StateMachineLoadError {
        if self.prompt_templates.is_empty() {
            return StateMachineLoadError::Invalid(format!(
                "state '{state_name}' references prompt template '{template_name}' but no prompt \
                 templates were loaded: add '{PROMPT_TEMPLATES_DIR}/{template_name}.md' beside \
                 'states.yaml' (state machines parsed from a string cannot resolve prompt \
                 templates, because there is no directory to read them from)"
            ));
        }
        let known = self.prompt_templates.keys().cloned().collect::<Vec<_>>().join(", ");
        StateMachineLoadError::Invalid(format!(
            "state '{state_name}' references unknown prompt template '{template_name}' \
             (declared templates: {known})"
        ))
    }
}

/// Reject the inline top-level `prompt_templates:` block that the sibling
/// directory replaced, so a stale machine fails loudly instead of silently
/// losing every prompt it declares.
// §FS-rhei-states.4.4: `states.yaml` must not declare a top-level prompt_templates block.
fn reject_inline_prompt_templates(raw: &serde_yaml::Value) -> Result<(), StateMachineLoadError> {
    if raw.get("prompt_templates").is_some() {
        return Err(StateMachineLoadError::Invalid(format!(
            "'prompt_templates' must be defined as a sibling '{PROMPT_TEMPLATES_DIR}/' directory \
             of Markdown files, not as a top-level field in 'states.yaml'"
        )));
    }
    Ok(())
}

fn reject_legacy_prompt_templates_file(path: &Path) -> Result<(), StateMachineLoadError> {
    let legacy_path =
        path.parent().unwrap_or_else(|| Path::new(".")).join(LEGACY_PROMPT_TEMPLATES_FILE);
    if legacy_path.exists() {
        return Err(StateMachineLoadError::Invalid(format!(
            "'{LEGACY_PROMPT_TEMPLATES_FILE}' is no longer supported; place prompt Markdown files \
             in sibling '{PROMPT_TEMPLATES_DIR}/'"
        )));
    }
    Ok(())
}

/// Read every `<id>.md` directly inside `prompt_templates/` into the machine.
///
/// A missing directory is not an error: prompt templates are opt-in.
fn load_prompt_templates_dir(
    path: &Path,
) -> Result<IndexMap<String, PromptTemplateDef>, StateMachineLoadError> {
    let mut templates = IndexMap::new();
    if !path.exists() {
        return Ok(templates);
    }
    if !path.is_dir() {
        return Err(StateMachineLoadError::Invalid(format!(
            "prompt_templates path '{}' must be a directory",
            path.display()
        )));
    }

    let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, std::io::Error>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let prompt_path = entry.path();
        if prompt_path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let template_name = prompt_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::trim)
            .filter(|stem| !stem.is_empty())
            .ok_or_else(|| {
                StateMachineLoadError::Invalid(format!(
                    "prompt template file '{}' must have a non-empty UTF-8 file stem",
                    prompt_path.display()
                ))
            })?
            .to_string();
        if templates.contains_key(&template_name) {
            return Err(StateMachineLoadError::Invalid(format!(
                "prompt_templates contains duplicate prompt template id '{template_name}'"
            )));
        }
        let instructions = std::fs::read_to_string(&prompt_path)?;
        templates.insert(template_name, PromptTemplateDef { instructions, source: Some(prompt_path) });
    }

    Ok(templates)
}

/// Replace `{placeholder}` tokens with the state's bound values.
///
/// Tokens without a bound value are left intact for the later runtime-variable
/// pass, and escaped braces are passed through still escaped so that pass is
/// the single place that unescapes them.
// §FS-rhei-states.4.4: values are substituted before runtime variables resolve.
fn substitute_prompt_template_values(text: &str, reference: &StatePromptTemplateRef) -> String {
    let mut rendered = String::with_capacity(text.len());
    let mut idx = 0usize;

    while idx < text.len() {
        if text[idx..].starts_with("\\{") || text[idx..].starts_with("\\}") {
            rendered.push_str(&text[idx..idx + 2]);
            idx += 2;
            continue;
        }
        if !text[idx..].starts_with('{') {
            let ch = text[idx..].chars().next().expect("substring should have a char");
            rendered.push(ch);
            idx += ch.len_utf8();
            continue;
        }
        let token_start = idx + 1;
        let Some(end_offset) = text[token_start..].find('}') else {
            rendered.push_str(&text[idx..]);
            break;
        };
        let end = token_start + end_offset;
        let token = &text[token_start..end];
        match reference.scalar_value(token) {
            Some(value) => rendered.push_str(&value),
            None => rendered.push_str(&text[idx..=end]),
        }
        idx = end + 1;
    }

    rendered
}

/// Join prompt fragments into one block, dropping the ones that carry no text.
fn join_prompt_parts<I>(parts: I) -> Option<String>
where
    I: IntoIterator<Item = Option<String>>,
{
    let text = parts
        .into_iter()
        .flatten()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn is_prompt_template_placeholder_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_prompt_template_scalar_value(value: &serde_yaml::Value) -> bool {
    matches!(
        value,
        serde_yaml::Value::Null
            | serde_yaml::Value::Bool(_)
            | serde_yaml::Value::Number(_)
            | serde_yaml::Value::String(_)
    )
}

/// Collect the `{...}` tokens in prompt text, skipping escaped braces so
/// `\{task_id\}` is not mistaken for a placeholder that needs a value.
fn extract_runtime_template_tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut idx = 0usize;
    while idx < text.len() {
        if text[idx..].starts_with("\\{") || text[idx..].starts_with("\\}") {
            idx += 2;
            continue;
        }
        if !text[idx..].starts_with('{') {
            let ch = text[idx..].chars().next().expect("substring should have a char");
            idx += ch.len_utf8();
            continue;
        }
        let token_start = idx + 1;
        let Some(end_offset) = text[token_start..].find('}') else {
            break;
        };
        let end = token_start + end_offset;
        tokens.push(&text[token_start..end]);
        idx = end + 1;
    }
    tokens
}

fn is_prompt_template_control_token(token: &str) -> bool {
    matches!(token, "else" | "endif") || token.starts_with("if ")
}
