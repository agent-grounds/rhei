// The gate as the live and static surfaces meet it: each waiting ticket's
// brief, built by the code `rhei gate show` runs, and an answer a surface
// posts, taken through the steps `rhei gate answer` takes. Neither surface
// holds a second implementation of either.

// §FS-rhei-viz.5.1 §FS-rhei-viz.8

/// The brief of every ticket at a gate, by task id: the document
/// `rhei gate show --json` prints for it, read the same way. A ticket whose
/// brief cannot be built is left out, and its surface falls back to the
/// machine's transitions. §FS-rhei-viz.8 §FS-rhei-viz.5.1.1
fn surface_gate_briefs(
    loaded: &LoadedPlan,
    input: &Path,
    machines: &rhei_validator::MachineSet,
) -> BTreeMap<String, rhei_viz_model::GateBrief> {
    let mut briefs = BTreeMap::new();
    for task in flatten_tasks(&loaded.rhei) {
        let source = GateViewSource { loaded, input, machine: machines.for_task(&task.id), task };
        let Ok(Some(view)) = gate_view(&source) else {
            continue;
        };
        let brief = serde_json::to_value(GateShowDocument::of(&view))
            .and_then(serde_json::from_value::<rhei_viz_model::GateBrief>);
        if let Ok(brief) = brief {
            briefs.insert(view.ticket.clone(), brief);
        }
    }
    briefs
}

/// Add the briefs to a static bundle: each plan in it gets the briefs of its
/// own tickets. Best effort, since a page renders without them; a plan the
/// gate commands cannot load keeps the machine's transitions. §FS-rhei-viz.8
fn attach_static_gate_briefs(
    plans: &mut BTreeMap<String, rhei_viz_model::VizModel>,
    input: &Path,
    state_machine: Option<&Path>,
) {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let Ok(loaded) = load_plan_leniently(input) else {
        return;
    };
    let Ok(resolved) = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine) else {
        return;
    };
    let briefs = surface_gate_briefs(&loaded, input, &resolved.validator_set());
    for model in plans.values_mut() {
        for task in &model.tasks {
            if let Some(brief) = briefs.get(&task.id) {
                model.gates.insert(task.id.clone(), brief.clone());
            }
        }
    }
}

/// One answer a live surface posts — the dashboard's `/answer-gate`, the run
/// TUI's gate form — through the steps `rhei gate answer` takes, with the
/// run's own machines and the answer document inline. The outcome is the
/// document `rhei gate answer --json` prints, refusals included; an `Err` is a
/// plan that could not be read at all. §FS-rhei-viz.5.1.2 §FS-rhei-gate-cmd.3
struct SurfaceAnswer<'a> {
    input: &'a Path,
    machines: &'a ExecutionMachines,
    no_callbacks: bool,
}

impl SurfaceAnswer<'_> {
    fn answer(&self, task_id: &str, from: &str, answers: &str, dry_run: bool) -> MietteResult<GateAnswerOutcome> {
        let loaded = load_plan(self.input)?;
        let task = find_task_by_id_str(&loaded.rhei.tasks, task_id)
            .ok_or_else(|| miette!(help = task_id_help(), "task '{}' not found in the plan", task_id))?;
        let mut outcome = GateAnswerOutcome::new(task_id, from, dry_run);
        let ask = GateAsk {
            from,
            answers: GateAnswerText::Inline(answers),
            dry_run,
            no_callbacks: self.no_callbacks,
            advise_live_run: false,
        };
        let scene = GateAnswerScene {
            ask,
            input: self.input,
            loaded: &loaded,
            machines: self.machines,
            task,
            task_id,
        };
        answer_gate(&scene, &mut outcome)?;
        outcome.ok = outcome.errors.is_empty();
        Ok(outcome)
    }
}
