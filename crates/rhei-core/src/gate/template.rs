// The template variables a gate's four text fields may use, and the one
// substitution that renders them. A gate runs nothing, so it has none of the
// variables an execution identity or a counted visit supplies.

// §FS-rhei-gate-brief.1.1

use std::path::{Component, Path};
use std::sync::OnceLock;

use regex::Regex;

/// The variables a `display` or `request` path may use. §FS-rhei-gate-brief.1.1
pub const PATH_VARIABLES: [&str; 4] = ["task_id", "task_id_local", "rhei_id", "state"];

/// The variables a `title` or `summary` adds to [`PATH_VARIABLES`], besides
/// `meta.<key>`. §FS-rhei-gate-brief.1.1
pub const PROSE_VARIABLES: [&str; 3] = ["task_title", "plan_title", "plan_path"];

/// Which of a gate's text fields a variable is checked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateField {
    Title,
    Summary,
    Display,
    Request,
}

impl GateField {
    pub fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Summary => "summary",
            Self::Display => "display",
            Self::Request => "request",
        }
    }

    fn is_path(self) -> bool {
        matches!(self, Self::Display | Self::Request)
    }

    /// Whether `variable` resolves in this field. §FS-rhei-gate-brief.1.1
    pub fn allows(self, variable: &str) -> bool {
        if PATH_VARIABLES.contains(&variable) {
            return true;
        }
        if self.is_path() {
            return false;
        }
        PROSE_VARIABLES.contains(&variable)
            || variable.strip_prefix("meta.").is_some_and(|key| !key.is_empty())
    }
}

/// An identifier-shaped `{name}`; anything else in braces is literal text.
fn variable_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"\{([A-Za-z_][A-Za-z0-9_.\-]*)\}").expect("variable pattern is valid")
    })
}

/// Every identifier-shaped variable `text` names, in order of appearance.
pub fn template_variables(text: &str) -> impl Iterator<Item = &str> {
    variable_pattern()
        .captures_iter(text)
        .filter_map(|captures| captures.get(1).map(|name| name.as_str()))
}

/// One problem per variable `field` does not allow, naming the field and the
/// variable. §FS-rhei-gate-brief.1.2
pub fn template_problems(field: GateField, text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for variable in template_variables(text) {
        if !field.allows(variable) {
            let allowed = if field.is_path() {
                "{task_id}, {task_id_local}, {rhei_id} and {state}"
            } else {
                "{task_id}, {task_id_local}, {rhei_id}, {state}, {task_title}, {plan_title}, \
                 {plan_path} and {meta.<key>}"
            };
            problems.push(format!(
                "`{}` uses {{{variable}}}, which a gate cannot resolve; it may use {allowed}",
                field.name()
            ));
        }
    }
    problems
}

/// Substitute each identifier-shaped variable `resolve` answers, leaving the
/// rest as written, as instructions are rendered. §FS-rhei-gate-brief.1.1
pub fn render_template(text: &str, resolve: impl Fn(&str) -> Option<String>) -> String {
    variable_pattern()
        .replace_all(text, |captures: &regex::Captures<'_>| {
            let whole = captures.get(0).map_or("", |whole| whole.as_str());
            captures
                .get(1)
                .and_then(|name| resolve(name.as_str()))
                .unwrap_or_else(|| whole.to_string())
        })
        .into_owned()
}

/// Why `path` cannot name a file under the execution root, if it cannot:
/// empty, absolute, or climbing out with `..`. §FS-rhei-gate-brief.1.2
pub fn relative_path_problem(path: &str) -> Option<&'static str> {
    if path.trim().is_empty() {
        return Some("is empty");
    }
    if crate::platform::path_is_rooted(path) || path.starts_with('/') || path.starts_with('\\') {
        return Some("is absolute; paths are relative to the execution root");
    }
    let mut depth = 0usize;
    for component in Path::new(path).components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Some("is absolute; paths are relative to the execution root")
            }
            Component::ParentDir => {
                if depth == 0 {
                    return Some("escapes the execution root with `..`");
                }
                depth -= 1;
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
        }
    }
    None
}
