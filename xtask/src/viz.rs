//! Dogfood the `rhei viz` static renderer ahead of (and alongside) the shipped
//! subcommand. This is now a thin wrapper: plan collection and the model live
//! in `rhei-viz`, and the single self-contained renderer lives in
//! `rhei-viz-model`. §AR-rhei-viz-flow.3, §AR-rhei-viz-flow.9 (step 1).

pub use rhei_cli::rhei_viz::{collect_plans, Bundle};

pub fn render_html(plans: &Bundle) -> String {
    rhei_cli::rhei_viz_model::render_static(plans)
}
