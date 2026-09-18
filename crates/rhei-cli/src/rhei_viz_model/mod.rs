//! Pure data model and the single self-contained HTML/CSS/JS renderer for the
//! Rhei **Flow** visualization — one asset, one model, byte-identical static
//! and live output, with no `rhei-core`/`rhei-validator` deps. §AR-rhei-viz-flow.2

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The self-contained Flow asset. The static path replaces [`BOOT_PLACEHOLDER`]
/// with the inlined plan bundle; the live path serves it verbatim (the
/// placeholder evaluates to `null`, which the JS reads as "poll `/snapshot`").
pub const FLOW_ASSET: &str = include_str!("../../assets/flow.html");

/// The JavaScript literal the renderer replaces with the inlined plan bundle.
const BOOT_PLACEHOLDER: &str = "/*__BOOT__*/null";

/// The §8 static base model: one plan's title, derived state, overview prose,
/// the flattened state machine, and the flat task list. The live `/snapshot`
/// payload is a superset of these fields (it adds the runtime overlay), so the
/// one asset renders both.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VizModel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_title: Option<String>,
    /// Derived plan state (§FS-rhei-viz §9). Carried for the supplementary
    /// surfaces; the Flow view itself reads per-task state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_state: Option<String>,
    /// Plan overview prose, rendered above the machine graphs as "what this
    /// Rhei is doing."
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// The flat task list: each top-level task and every descendant, carrying
    /// its `parent`/`depth` rather than nesting. §FS-rhei-viz §8
    pub tasks: Vec<TaskRow>,
    /// The resolved state machine, flattened for the surface. §FS-rhei-viz §8
    pub machine: Machine,
}

/// One node in the flat task list: a top-level task or any descendant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: String,
    pub title: String,
    /// Parent id, or `None` for a top-level task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Id depth: `0` for a top-level task, `1` for its child, and so on.
    pub depth: u8,
    pub state: String,
    /// Explicit counted-loop visit from the raw markdown state, when present.
    /// The public `state` remains canonical so categorization and machine lookup
    /// stay stable while prompts/artifact paths can render the active visit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_count: Option<u32>,
    /// Prerequisite task ids (`**Prior:**`).
    #[serde(default)]
    pub prior: Vec<String>,
    /// Durable state-transition history parsed from the central runtime ledger.
    /// Renderers use it for the selected task's bounded previous-states section.
    /// §FS-rhei-viz.4 §FS-rhei-viz.8
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<StateHistoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateHistoryEntry {
    /// Exceptional audit reason for this one movement. §FS-rhei-viz.4
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forced_reason: Option<String>,
    pub from: String,
    pub to: String,
}

/// The resolved state machine, flattened so the surface can show, per state,
/// the allowed transitions and the input/output artifact contracts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    pub states: Vec<MachineState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MachineState {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The agent prompt for this state, with template variables unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Counted-loop budget, when the state declares one (`visits: N`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visits: Option<u32>,
    /// True when this state is the entry of at least one profile (the union over
    /// profiles, since initiality is per-profile). §FS-rhei-states
    pub initial: bool,
    pub terminal: bool,
    pub gating: bool,
    /// The person this state's poll waits on, when it declares one. The
    /// renderers classify on it, so it has to cross the wire: without it the
    /// dashboard and the TUI would read an approval wait as live work while
    /// `rhei list` read it as a person's turn. §FS-rhei-states.2.5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_on: Option<String>,
    /// Authored autonomous process kind for this state, when the state machine
    /// declares one. Renderers use this to distinguish agent and program states.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<MachineProcessKind>,
    /// Allowed outgoing transitions: explicit edges first, then `from: "*"`
    /// wildcard edges that apply to this non-terminal state.
    pub transitions: Vec<Transition>,
    #[serde(default)]
    pub inputs: Vec<Artifact>,
    #[serde(default)]
    pub outputs: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "TemplateContext::is_empty")]
    pub template_context: TemplateContext,
    /// Concrete authored fanout contexts for static prompt/artifact previews.
    /// Live renders prefer the running slot context because it reflects the
    /// invocation that actually owns the task. §FS-rhei-viz.8
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_contexts: Vec<TemplateContext>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MachineProcessKind {
    Agent,
    Program,
}

/// Static template values the renderer may resolve without guessing. Ambiguous
/// fields stay absent unless represented by `template_contexts`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TemplateContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_mode: Option<String>,
}

impl TemplateContext {
    fn is_empty(&self) -> bool {
        self.target.is_none()
            && self.target_slug.is_none()
            && self.model.is_none()
            && self.model_provider.is_none()
            && self.model_name.is_none()
            && self.agent.is_none()
            && self.agent_mode.is_none()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transition {
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// True when this edge comes from a `from: "*"` wildcard rule.
    pub wildcard: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub optional: bool,
}

/// Render a self-contained static page from a bundle of plans keyed by a short
/// name (the JS shows a selector when there is more than one). Produced by
/// `rhei viz`, the `xtask` dogfood, and the run-end freeze. §FS-rhei-viz.7.2
pub fn render_static(plans: &BTreeMap<String, VizModel>) -> String {
    let data = serde_json::to_string(plans).expect("plan bundle should always serialize");
    render_inline(&data)
}

/// Render from an already-serialized boot bundle (plan key -> plan). Used by
/// [`render_static`] and the run-end freeze, which inlines the final superset
/// snapshot so the frozen page equals the live one minus polling. §AR-rhei-viz-flow.5.3
pub fn render_inline(boot_json: &str) -> String {
    FLOW_ASSET.replace(BOOT_PLACEHOLDER, &escape_json_for_html_script(boot_json))
}

/// Prepend a visible caveat banner to a rendered page. A limitation announced
/// only on stderr is gone once the terminal scrolls, so anything that makes the
/// graph incomplete must travel inside the file. §FS-rhei-viz.7.3
pub fn with_page_notice(html: &str, notice: &str) -> String {
    let banner = format!(
        "<div role=\"note\" style=\"padding:10px 16px;background:var(--surface-2);\
         border-bottom:1px solid var(--hairline);color:var(--dim);\
         font:12px/1.5 var(--mono)\">{}</div>",
        escape_html_text(notice)
    );
    match html.find("<body>") {
        Some(index) => {
            let split = index + "<body>".len();
            format!("{}\n{}{}", &html[..split], banner, &html[split..])
        }
        // No recognizable body tag: prepending still keeps the notice visible
        // rather than dropping it.
        None => format!("{banner}{html}"),
    }
}

fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Convenience wrapper to render a single plan under a given key.
pub fn render_static_one(key: &str, model: &VizModel) -> String {
    let mut bundle = BTreeMap::new();
    bundle.insert(key.to_string(), model.clone());
    render_static(&bundle)
}

/// The asset string for the **live** surface: served verbatim at `/`, with the
/// `BOOT` placeholder left as `null` so the JS polls `/snapshot` instead of
/// reading an inlined bundle. AR §2, §5.2.
pub fn live_asset() -> &'static str {
    FLOW_ASSET
}

/// Escape a JSON string for safe inlining inside an HTML `<script>` element:
/// neutralizes `</script>` breakouts and the two Unicode line terminators that
/// are illegal in JS string literals.
pub fn escape_json_for_html_script(data: &str) -> String {
    let mut out = String::with_capacity(data.len());
    for ch in data.chars() {
        match ch {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_model() -> VizModel {
        VizModel {
            plan_title: Some("Demo".into()),
            plan_state: Some("active".into()),
            about: None,
            tasks: vec![TaskRow {
                id: "1".into(),
                title: "Alpha".into(),
                parent: None,
                depth: 0,
                state: "in-progress".into(),
                visit_count: None,
                prior: vec![],
                history: vec![],
            }],
            machine: Machine { name: "rhei".into(), states: vec![] },
        }
    }

    #[test]
    fn render_static_inlines_bundle_and_drops_placeholder() {
        let html = render_static_one("demo", &demo_model());
        assert!(!html.contains(BOOT_PLACEHOLDER));
        assert!(html.contains("\"plan_title\":\"Demo\""));
        assert!(html.contains("\"depth\":0"));
    }

    #[test]
    fn render_static_escapes_script_breakouts() {
        let mut model = demo_model();
        model.plan_title = Some("</script><script>alert(1)</script>".into());
        let html = render_static_one("demo", &model);
        assert!(!html.contains("</script><script>alert(1)</script>"));
        assert!(
            html.contains("\\u003c/script\\u003e\\u003cscript\\u003ealert(1)\\u003c/script\\u003e")
        );
    }

    #[test]
    fn live_asset_leaves_boot_null_so_js_polls() {
        assert!(live_asset().contains(BOOT_PLACEHOLDER));
    }

    #[test]
    fn flow_asset_running_now_uses_runtime_slots() {
        assert!(FLOW_ASSET.contains("function isRunningNow(n)"));
        assert!(FLOW_ASSET.contains("filter(isRunningNow)"));
        assert!(FLOW_ASSET.contains("const runningPart = hasRuntimeOverlay()"));
    }

    /// The dashboard's `category()` mirrors the Rust one, so the three
    /// surfaces cannot disagree about a poll that waits on a person.
    // §FS-rhei-viz.1.1 §FS-rhei-states.2.5
    #[test]
    fn flow_asset_classifies_a_person_waiting_poll_as_a_gate() {
        assert!(FLOW_ASSET.contains(r#"if (ms && ms.waiting_on) return "gate";"#));
    }

    /// The pause glyph says the node is somebody's turn; both flag lines say
    /// whose, so the label the wire carries is one an operator can read.
    // §FS-rhei-viz.4 §FS-rhei-viz.6
    #[test]
    fn flow_asset_shows_the_person_a_poll_waits_on() {
        assert!(FLOW_ASSET.contains(r#"flags.push("waiting on "+ms.waiting_on)"#));
        assert!(FLOW_ASSET.contains(r#"flags.push(`waiting on ${ms.waiting_on}`)"#));
    }

    #[test]
    fn flow_asset_clears_stale_running_overlay_on_poll_loss() {
        assert!(FLOW_ASSET.contains("let POLL_FAILS=0, LIVE_STALE=false"));
        assert!(FLOW_ASSET.contains("function clearRunningOverlay()"));
        assert!(FLOW_ASSET.contains("if(POLL_FAILS>=2) clearRunningOverlay()"));
        assert!(FLOW_ASSET.contains("delete rt.in_slot"));
        assert!(FLOW_ASSET.contains("s.active=false"));
    }
}
