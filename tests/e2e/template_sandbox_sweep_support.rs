//! What each shipped template would launch, worked out from the plan it lays.
//!
//! A search of the templates for a flag finds only the modes they spell out. A
//! state that names `codex` and no mode inherits the built-in's first mode, and
//! that is a launch no search sees, so the sweep resolves every state the way
//! `rhei validate` does and reads the flags off what it resolved.
//! §FS-rhei-templates.4.1 §FS-rhei-agents.1.4.1

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::template_timeout_tests::{required_inputs, shipped_templates};
use super::templates_tests::run_raw;
use super::*;

/// The built-in `codex` profile's `workspace` mode, flag for flag. §FS-rhei-agents.2
pub(super) const WORKSPACE_FLAGS: [&str; 7] = [
    "--sandbox",
    "workspace-write",
    "--skip-git-repo-check",
    "-c",
    "approval_policy=\"never\"",
    "-c",
    "sandbox_workspace_write.network_access=true",
];

/// The built-in's first mode, which a state naming no mode gets. The roster
/// sorts mode names, so it cannot say which is first; the launch guard in
/// `codex_writable_roots_tests` pins this. §FS-rhei-agents.2 §FS-rhei-agents.1.4.1
const BUILT_IN_FIRST_MODE: &str = "yolo";

/// One shipped template laid into its own directory, with what the roster says
/// its effective agents and defaults are.
pub(super) struct LaidTemplate {
    pub template: String,
    /// `defaults`, or the values file it was laid with.
    pub values: &'static str,
    pub root: PathBuf,
    pub roster: Value,
}

/// One launch a state of a laid template would make.
pub(super) struct Launch {
    pub state: String,
    pub agent: String,
    pub mode: Option<String>,
    pub argv: Vec<String>,
}

impl LaidTemplate {
    fn profile(&self, agent: &str) -> &Value {
        &self.roster["agents"][agent]
    }

    /// Whether `agent` resolves to the `codex` family or runs the `codex`
    /// binary. §FS-rhei-templates.4.1
    pub fn is_codex(&self, agent: &str) -> bool {
        let profile = self.profile(agent);
        let family = profile["family"].as_str().unwrap_or(agent);
        let binary = profile["command"][0].as_str().unwrap_or_default();
        family == "codex" || Path::new(binary).file_stem().is_some_and(|stem| stem == "codex")
    }

    /// The profile's modes, from the roster.
    pub fn modes(&self, agent: &str) -> serde_json::Map<String, Value> {
        self.profile(agent)["modes"].as_object().cloned().unwrap_or_default()
    }

    /// Whether the profile resolves a non-empty `writable_roots.path_flag`.
    pub fn has_writable_roots(&self, agent: &str) -> bool {
        self.profile(agent)["writable_roots"]["path_flag"]
            .as_str()
            .is_some_and(|flag| !flag.is_empty())
    }

    /// Every Codex profile the plan resolves, by id.
    pub fn codex_profiles(&self) -> Vec<String> {
        let agents = self.roster["agents"].as_object().expect("roster agents");
        agents.keys().filter(|agent| self.is_codex(agent)).cloned().collect()
    }

    pub fn label(&self) -> String {
        format!("{} ({})", self.template, self.values)
    }
}

/// Lay every shipped template with its defaults, and again with its
/// `.example-values.yaml` where it ships one. §FS-rhei-templates.4.1
pub(super) fn lay_every_shipped_template(dir: &Path) -> Vec<LaidTemplate> {
    let mut laid = Vec::new();
    for (template, source) in shipped_templates() {
        laid.push(lay(dir, &template, &source, None, &[]));
        let values = source.join(".example-values.yaml");
        if values.is_file() {
            laid.push(lay(dir, &template, &source, Some(&values), &[]));
        }
    }
    laid
}

/// Lay `source` into `dir`, giving a placeholder to every required input that
/// neither `values` nor `extra` sets, and read its roster.
pub(super) fn lay(
    dir: &Path,
    template: &str,
    source: &Path,
    values: Option<&Path>,
    extra: &[&str],
) -> LaidTemplate {
    let label = if values.is_some() { ".example-values.yaml" } else { "defaults" };
    let tag = if values.is_some() { "example" } else { "defaults" };
    let output = format!("out-{template}-{tag}-{}", extra.len());
    let source_arg = source.to_string_lossy().into_owned();
    let given = values.map(values_keys).unwrap_or_default();
    let mut args = vec!["instantiate".to_string(), source_arg.clone()];
    if let Some(values) = values {
        args.push("--values".to_string());
        args.push(values.to_string_lossy().into_owned());
    }
    args.extend(extra.iter().map(|arg| (*arg).to_string()));
    for input in required_inputs(dir, &source_arg) {
        let name = input.split('=').next().unwrap_or_default();
        let set_by_extra = extra.iter().any(|arg| arg.starts_with(&format!("{name}=")));
        if !given.iter().any(|key| key == name) && !set_by_extra {
            args.push(input);
        }
    }
    args.push("--output".to_string());
    args.push(output.clone());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let laid = run_raw(&args, dir);
    assert!(
        laid.status.success(),
        "template '{template}' ({label}) should instantiate\nstdout:\n{}\nstderr:\n{}",
        laid.stdout,
        laid.stderr
    );
    let roster = run_raw(&["roster", &output, "--json"], dir);
    assert_success(&roster);
    let roster = serde_json::from_str(&roster.stdout).expect("roster JSON should parse");
    LaidTemplate { template: template.to_string(), values: label, root: dir.join(output), roster }
}

fn values_keys(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).expect("read the values file");
    let value: serde_yaml::Value = serde_yaml::from_str(&text).expect("values file is YAML");
    let mapping = value.as_mapping().cloned().unwrap_or_default();
    mapping.keys().filter_map(|key| key.as_str().map(str::to_string)).collect()
}

/// Every launch the laid plan's states and tasks would make.
pub(super) fn launches(laid: &LaidTemplate) -> Vec<Launch> {
    let mut launches = Vec::new();
    for machine in files_named(&laid.root, "states.yaml") {
        let text = fs::read_to_string(&machine).expect("read a laid states.yaml");
        let doc: serde_yaml::Value = serde_yaml::from_str(&text).expect("laid states.yaml parses");
        let Some(states) = doc["states"].as_mapping() else { continue };
        for (name, state) in states {
            let name = name.as_str().unwrap_or_default().to_string();
            for (agent, mode, model) in state_selections(laid, state) {
                launches.push(launch(laid, &name, agent, mode, model));
            }
        }
    }
    for task in files_with_extension(&laid.root.join("tasks"), "md") {
        let text = fs::read_to_string(&task).expect("read a laid task");
        for selector in text.lines().filter_map(|line| line.strip_prefix("**Target:**")) {
            let (agent, mode, model) = parse_selector(selector.trim());
            launches.push(launch(laid, &format!("task {}", task.display()), agent, mode, model));
        }
    }
    launches
}

fn launch(
    laid: &LaidTemplate,
    state: &str,
    agent: String,
    mode: Option<String>,
    model: Option<String>,
) -> Launch {
    let profile = laid.profile(&agent);
    let mut argv: Vec<String> = strings(&profile["command"]);
    if let Some(mode) = &mode {
        argv.extend(strings(&profile["modes"][mode]));
    }
    if let Some(model) = &model {
        argv.extend(strings(&laid.roster["models"][model]["agents"][&agent]["autonomous_args"]));
    }
    Launch { state: state.to_string(), agent, mode, argv }
}

/// The selections one state makes: its selectors, or else its agent and the
/// mode §FS-rhei-agents.1.4.1 picks for it without a CLI override.
fn state_selections(
    laid: &LaidTemplate,
    state: &serde_yaml::Value,
) -> Vec<(String, Option<String>, Option<String>)> {
    let mut selectors: Vec<String> = Vec::new();
    if let Some(target) = state["target"].as_str() {
        selectors.push(target.to_string());
    }
    if let Some(all) = state["all_targets"].as_sequence() {
        selectors.extend(all.iter().filter_map(|t| t.as_str().map(str::to_string)));
    }
    if !selectors.is_empty() {
        return selectors.iter().map(|selector| parse_selector(selector)).collect();
    }
    let defaults = &laid.roster["defaults"];
    let passive = state["final"].as_bool() == Some(true)
        || state["gating"].as_bool() == Some(true)
        || !state["program"].is_null();
    let agent = match state["agent"].as_str() {
        Some(agent) => agent.to_string(),
        None if !passive => match defaults["agent"].as_str() {
            Some(agent) => agent.to_string(),
            None => return Vec::new(),
        },
        None => return Vec::new(),
    };
    let mode = state["agent_mode"]
        .as_str()
        .or_else(|| defaults["agent_mode"].as_str())
        .map(str::to_string)
        .or_else(|| first_mode(laid, &agent));
    vec![(agent, mode, None)]
}

/// The first mode `agent` declares: the first key of the laid settings' own
/// `modes` when it writes one, and otherwise the built-in's.
fn first_mode(laid: &LaidTemplate, agent: &str) -> Option<String> {
    let settings = laid.root.join(".agent-grounds/rhei/settings.json");
    if let Ok(text) = fs::read_to_string(&settings) {
        // YAML keeps the file's key order, which a `serde_json::Map` does not.
        let doc: serde_yaml::Value = serde_yaml::from_str(&text).expect("settings parse");
        if let Some(modes) = doc["agents"][agent]["modes"].as_mapping() {
            return modes.keys().next().and_then(|key| key.as_str()).map(str::to_string);
        }
    }
    let modes = laid.modes(agent);
    if modes.contains_key(BUILT_IN_FIRST_MODE) {
        return Some(BUILT_IN_FIRST_MODE.to_string());
    }
    modes.keys().next().cloned()
}

/// `agent[mode]:provider:model`, mode optional.
pub(super) fn parse_selector(selector: &str) -> (String, Option<String>, Option<String>) {
    let mut parts = selector.splitn(3, ':');
    let head = parts.next().unwrap_or_default();
    let model = parts.nth(1).map(str::to_string);
    match head.split_once('[') {
        Some((agent, rest)) => {
            (agent.to_string(), Some(rest.trim_end_matches(']').to_string()), model)
        }
        None => (head.to_string(), None, model),
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Every file called `name` under `root`, leaving out `runtime/`.
pub(super) fn files_named(root: &Path, name: &str) -> Vec<PathBuf> {
    walk(root).into_iter().filter(|path| path.file_name().is_some_and(|n| n == name)).collect()
}

fn files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    walk(root).into_iter().filter(|path| path.extension().is_some_and(|e| e == extension)).collect()
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name != "runtime") {
                    pending.push(path);
                }
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
