use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use crate::rhei_viz_model::{Machine, MachineState, TaskRow, Transition, VizModel};
use crossterm::event::{KeyCode, KeyModifiers};

use super::input::{handle_key_event, InputAction};
use super::state::{CostGroup, FlowFocus, UiState, UsageRecord, View};
use super::text::{sanitize_terminal_text, truncate_chars};
use super::theme;
use super::{leave_finished_screen, message_goes_to_stderr};
use crate::rhei_tui::dashboard::InterveneSink;
use crate::rhei_tui::event::{
    AgentStream, DimensionStatus, DimensionSummary, MessageLevel, PricingStatus, RunEvent, Slot,
    TaskOutcome, UsageCoverage, UsageReport, UsageStatus, UsageSummary,
};

fn machine_state(name: &str, gating: bool, transitions: Vec<&str>) -> MachineState {
    MachineState {
        name: name.to_string(),
        description: None,
        instructions: None,
        visits: None,
        initial: name == "draft",
        terminal: matches!(name, "completed" | "done" | "cancelled"),
        gating,
        waiting_on: None,
        process: None,
        transitions: transitions
            .into_iter()
            .map(|to| Transition { to: to.to_string(), condition: None, wildcard: false })
            .collect(),
        inputs: vec![],
        outputs: vec![],
        template_context: Default::default(),
        template_contexts: vec![],
    }
}

/// The terminal reads the same classification off the flattened machine as the
/// browser and `rhei viz` do — a person-waiting poll is a pause, a CI watch is
/// still active. §FS-rhei-viz.1.1 §FS-rhei-states.2.5
#[test]
fn a_poll_waiting_on_a_person_reads_as_a_pause_in_the_terminal() {
    let mut approval = machine_state("plan-approval", false, vec!["plan-approval", "ci-watch"]);
    approval.waiting_on = Some("author".to_string());
    let machine = Machine {
        name: "approvals".into(),
        states: vec![approval, machine_state("ci-watch", false, vec!["ci-watch"])],
    };

    assert_eq!(theme::category(&machine, "plan-approval"), theme::Category::Gate);
    assert_eq!(theme::category(&machine, "ci-watch"), theme::Category::Active);
}

/// The pause color says the ticket is somebody's turn; the flags line is where
/// the terminal says whose — in the task inspector (§FS-rhei-viz.4) and in the
/// state detail, which adds only its counted budget (§FS-rhei-viz.6).
#[test]
fn the_inspector_flags_name_the_person_a_poll_waits_on() {
    let mut state = state_with_plan();
    state.plan.machine.states[1].waiting_on = Some("author".to_string());
    let task = state.plan.tasks[0].clone();
    assert_eq!(task.state, "in-progress", "the fixture's first task sits in the second state");
    assert!(super::views::task_flags(&state, &task).contains("waiting on author"));
    let detail = super::views::machine_state_flags(&state.plan.machine.states[1], true);
    assert!(detail.contains(&"waiting on author".to_string()), "{detail:?}");
}

fn demo_model() -> VizModel {
    VizModel {
        plan_title: Some("Demo".into()),
        plan_state: Some("active".into()),
        about: None,
        tasks: vec![
            TaskRow {
                id: "1".into(),
                title: "Alpha".into(),
                parent: None,
                depth: 0,
                state: "in-progress".into(),
                visit_count: None,
                prior: vec![],
                history: vec![],
            },
            TaskRow {
                id: "2".into(),
                title: "Beta".into(),
                parent: None,
                depth: 0,
                state: "human-review".into(),
                visit_count: None,
                prior: vec!["1".into()],
                history: vec![],
            },
        ],
        machine: Machine {
            name: "rhei".into(),
            states: vec![
                machine_state("draft", false, vec!["in-progress"]),
                machine_state("in-progress", false, vec!["human-review"]),
                machine_state("human-review", true, vec!["completed", "in-progress"]),
                machine_state("completed", false, vec![]),
            ],
        },
    }
}

fn state_with_plan() -> UiState {
    let mut state = UiState::with_context(PathBuf::from("/ws"), 2, 2, None, None, None, false);
    state.plan = demo_model();
    state.refresh_plan();
    state
}

fn press(state: &mut UiState, code: KeyCode) -> InputAction {
    handle_key_event(state, code, KeyModifiers::NONE)
}

#[test]
fn ctrl_c_requests_sigint_forwarding() {
    let mut state = state_with_plan();
    let action = handle_key_event(&mut state, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(matches!(action, InputAction::ForwardSigint));
}

#[test]
fn quit_only_after_finished() {
    let mut state = state_with_plan();
    assert!(matches!(press(&mut state, KeyCode::Char('q')), InputAction::Continue));
    state.finished = true;
    assert!(matches!(press(&mut state, KeyCode::Char('q')), InputAction::Quit));
}

#[test]
fn number_keys_switch_views() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('2'));
    assert!(state.view == View::Machine);
    press(&mut state, KeyCode::Char('4'));
    assert!(state.view == View::Journal);
    press(&mut state, KeyCode::Char('1'));
    assert!(state.view == View::Flow);
}

#[test]
fn auto_selects_first_active_task() {
    let state = state_with_plan();
    // Task 1 is `in-progress` → state-derived active; selected on load.
    assert_eq!(state.selected.as_deref(), Some("1"));
}

#[test]
fn outline_movement_changes_selection() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('j'));
    assert_eq!(state.selected.as_deref(), Some("2"));
    press(&mut state, KeyCode::Char('k'));
    assert_eq!(state.selected.as_deref(), Some("1"));
}

#[test]
fn tab_toggles_flow_focus() {
    let mut state = state_with_plan();
    assert!(matches!(state.flow_focus, FlowFocus::Outline));
    press(&mut state, KeyCode::Tab);
    assert!(matches!(state.flow_focus, FlowFocus::Inspector));
}

#[test]
fn filter_narrows_visible_tasks() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('/'));
    press(&mut state, KeyCode::Char('B'));
    press(&mut state, KeyCode::Char('e'));
    press(&mut state, KeyCode::Enter);
    let visible = state.visible_task_indices();
    assert_eq!(visible.len(), 1);
    assert_eq!(state.plan.tasks[visible[0]].id, "2");
}

#[test]
fn filter_narrows_machine_states_and_keeps_focus_visible() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('2'));
    press(&mut state, KeyCode::Char('/'));
    for ch in "human".chars() {
        press(&mut state, KeyCode::Char(ch));
    }
    press(&mut state, KeyCode::Enter);

    let states = state
        .machine_view_order()
        .iter()
        .map(|i| state.plan.machine.states[*i].name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(states, vec!["human-review"]);
    assert_eq!(state.plan.machine.states[state.machine_focus].name, "human-review");

    press(&mut state, KeyCode::Char('j'));
    assert_eq!(state.plan.machine.states[state.machine_focus].name, "human-review");
}

#[test]
fn filter_narrows_journal_lines() {
    let mut state = state_with_plan();
    state.push_journal(MessageLevel::Info, "alpha transition".into());
    state.push_journal(MessageLevel::Warn, "beta warning".into());
    press(&mut state, KeyCode::Char('4'));
    press(&mut state, KeyCode::Char('/'));
    for ch in "beta".chars() {
        press(&mut state, KeyCode::Char(ch));
    }
    press(&mut state, KeyCode::Enter);

    let filtered = state.filtered_journal().iter().map(|e| e.text.as_str()).collect::<Vec<_>>();
    assert_eq!(filtered, vec!["beta warning"]);
}

#[test]
fn agent_output_is_recorded_on_slot() {
    let mut state = state_with_plan();
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: Some("codex".into()),
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    state.apply(&RunEvent::AgentOutput {
        slot: 0,
        task: "1".into(),
        stream: AgentStream::Stdout,
        line: "hello".into(),
        wall_clock: SystemTime::now(),
    });
    assert!(state.is_live("1"));
    assert_eq!(state.slots[0].traffic.back().map(|t| t.text.as_str()), Some("hello"));
}

#[test]
fn agent_output_logical_lines_remain_separate_in_live_traffic() {
    // §FS-rhei-run-tui.1.2: line-oriented events must not concatenate a
    // multiline structured result when the TUI sanitizes live traffic.
    let mut state = state_with_plan();
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: Some("claude-code".into()),
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    for line in ["first", "", "second"] {
        state.apply(&RunEvent::AgentOutput {
            slot: 0,
            task: "1".into(),
            stream: AgentStream::Stdout,
            line: line.into(),
            wall_clock: SystemTime::now(),
        });
    }

    let traffic = state.slots[0].traffic.iter().map(|line| line.text.as_str()).collect::<Vec<_>>();
    assert_eq!(traffic, ["first", "", "second"]);
}

#[test]
fn slot_release_clears_live_marker() {
    let mut state = state_with_plan();
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: None,
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    assert!(state.is_live("1"));
    state.apply(&RunEvent::SlotReleased {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "human-review".into(),
        log_path: PathBuf::from("1.log"),
        outcome: TaskOutcome::Completed,
        finished_at: Instant::now(),
        wall_clock: SystemTime::now(),
        exit_code: Some(0),
        duration_ms: 1200,
    });
    assert!(!state.is_live("1"));
}

/// A provider wait is calm terminal chrome for the released slot and keeps
/// its provider/deadline visible to an attached operator. §FS-rhei-run-tui.1.1
#[test]
fn provider_limit_release_is_a_calm_visible_wait() {
    let mut state = state_with_plan();
    state.apply(&RunEvent::SlotReleased {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        log_path: PathBuf::from("1.log"),
        outcome: TaskOutcome::ProviderLimited {
            provider: "openai".into(),
            next_attempt_at: "2026-09-16T20:21:00Z".into(),
        },
        finished_at: Instant::now(),
        wall_clock: SystemTime::now(),
        exit_code: Some(1),
        duration_ms: 1200,
    });

    let entry = state.journal.back().expect("release journal entry");
    assert_eq!(entry.level, MessageLevel::Info);
    assert!(entry.text.contains("⏳ slot 0: 1 openai until 2026-09-16T20:21:00Z"));
}

#[test]
fn journal_filter_keeps_only_warnings() {
    let mut state = state_with_plan();
    state.push_journal(MessageLevel::Info, "info".into());
    state.push_journal(MessageLevel::Warn, "warn".into());
    state.journal_filter = super::state::JournalFilter::Warnings;
    let filtered: Vec<_> = state.filtered_journal().iter().map(|e| e.text.clone()).collect();
    assert_eq!(filtered, vec!["warn".to_string()]);
}

#[test]
fn dashboard_link_is_pinned_in_header() {
    let mut state = state_with_plan();
    state.apply(&RunEvent::RunLink {
        label: "Dashboard".into(),
        url: "http://127.0.0.1:54321".into(),
    });
    assert_eq!(state.dashboard_url.as_deref(), Some("http://127.0.0.1:54321"));
}

#[test]
fn gate_choices_list_explicit_transitions() {
    let mut state = state_with_plan();
    state.select_task("2"); // human-review, gating
    let choices = super::input::gate_choices(&state);
    assert_eq!(choices.len(), 2);
    assert!(choices.iter().any(|(_, to)| to == "completed"));
}

struct ReachableSink {
    delivered: Mutex<Vec<String>>,
}

impl InterveneSink for ReachableSink {
    fn deliver(
        &self,
        _task_id: Option<&str>,
        _slot: Option<Slot>,
        message: &str,
    ) -> Result<(), String> {
        self.delivered.lock().unwrap().push(message.to_string());
        Ok(())
    }
    fn reachable(&self, _task_id: &str, _slot: Option<Slot>) -> bool {
        true
    }
}

#[test]
fn intervene_composer_delivers_message() {
    let sink = Arc::new(ReachableSink { delivered: Mutex::new(Vec::new()) });
    let mut state = state_with_plan();
    state.intervene = Some(sink.clone());
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: Some("claude-code".into()),
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    state.select_task("1");
    press(&mut state, KeyCode::Char('m'));
    assert!(state.composer.is_some());
    for ch in "ping".chars() {
        press(&mut state, KeyCode::Char(ch));
    }
    press(&mut state, KeyCode::Enter);
    assert!(state.composer.is_none());
    assert_eq!(sink.delivered.lock().unwrap().as_slice(), ["ping".to_string()]);
}

#[test]
fn intervene_unreachable_when_not_live() {
    let mut state = state_with_plan();
    state.select_task("1");
    press(&mut state, KeyCode::Char('m'));
    // No running slot for task 1 → composer must not open.
    assert!(state.composer.is_none());
}

fn measured(value: u64) -> DimensionSummary {
    DimensionSummary {
        value: Some(value),
        status: DimensionStatus::Measured,
        missing_count: 0,
        measured_count: 1,
    }
}

fn demo_usage() -> UsageSummary {
    UsageSummary {
        invocation_id: "inv-1".into(),
        state: "in-progress".into(),
        agent: "claude-code".into(),
        provider: Some("anthropic".into()),
        model: Some("claude".into()),
        total: measured(1200),
        input_total: measured(1000),
        input_cached_read: measured(400),
        input_cache_write: measured(0),
        output_total: measured(200),
        output_cached_read: measured(0),
        output_cache_write: measured(0),
        cost_micro: Some(2_500_000),
        priced_cost_micro: Some(2_500_000),
        currency: Some("USD".into()),
        coverage: UsageCoverage::Complete,
        status: UsageStatus::Measured,
        pricing_status: PricingStatus::Priced,
    }
}

#[test]
fn cost_state_grouping_uses_invocation_state() {
    let mut state = state_with_plan();
    let mut usage = demo_usage();
    usage.cost_micro = Some(100);
    usage.priced_cost_micro = Some(100);
    state.invocations.push(UsageRecord { task: "1".into(), usage });

    let mut usage = demo_usage();
    usage.invocation_id = "inv-2".into();
    usage.state = "human-review".into();
    usage.cost_micro = Some(200);
    usage.priced_cost_micro = Some(200);
    state.invocations.push(UsageRecord { task: "2".into(), usage });
    state.cost_group = CostGroup::State;

    let rows = super::views::cost_rows(&state);

    assert_eq!(
        rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        vec!["human-review", "in-progress",]
    );
    assert_eq!(
        rows.iter().map(|row| row.1.cost_micro).collect::<Vec<_>>(),
        vec![Some(200), Some(100),]
    );
}

#[test]
fn task_readiness_uses_machine_terminal_priors() {
    let mut state = state_with_plan();
    state.plan.tasks = vec![
        TaskRow {
            id: "1".into(),
            title: "Done".into(),
            parent: None,
            depth: 0,
            state: "done".into(),
            visit_count: None,
            prior: vec![],
            history: vec![],
        },
        TaskRow {
            id: "2".into(),
            title: "Dependent".into(),
            parent: None,
            depth: 0,
            state: "draft".into(),
            visit_count: None,
            prior: vec!["1".into()],
            history: vec![],
        },
    ];
    state.plan.machine.states.push(machine_state("done", false, vec![]));
    let dependent = &state.plan.tasks[1];

    assert_eq!(state.task_ready(dependent), "ready");
    assert!(state.unresolved_priors(dependent).is_empty());
}

#[test]
fn cancelled_terminal_priors_do_not_unblock_tasks() {
    let mut state = state_with_plan();
    state.plan.tasks = vec![
        TaskRow {
            id: "1".into(),
            title: "Cancelled".into(),
            parent: None,
            depth: 0,
            state: "cancelled".into(),
            visit_count: None,
            prior: vec![],
            history: vec![],
        },
        TaskRow {
            id: "2".into(),
            title: "Dependent".into(),
            parent: None,
            depth: 0,
            state: "draft".into(),
            visit_count: None,
            prior: vec!["1".into()],
            history: vec![],
        },
    ];
    state.plan.machine.states.push(machine_state("cancelled", false, vec![]));
    let dependent = &state.plan.tasks[1];

    assert_eq!(state.task_ready(dependent), "blocked");
    assert_eq!(state.unresolved_priors(dependent), vec!["1".to_string()]);
}

#[test]
fn renders_every_view_and_overlay_without_panic() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let mut state = state_with_plan();
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: Some("claude-code".into()),
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    state.apply(&RunEvent::AgentOutput {
        slot: 0,
        task: "1".into(),
        stream: AgentStream::Stdout,
        line: "working…".into(),
        wall_clock: SystemTime::now(),
    });
    state.apply(&RunEvent::UsageReported {
        slot: Some(0),
        task: "1".into(),
        invocation_id: "inv-1".into(),
        report: UsageReport::Final,
        usage: demo_usage(),
    });
    state.apply(&RunEvent::RunLink {
        label: "Dashboard".into(),
        url: "http://127.0.0.1:5000".into(),
    });

    // Wide, narrow-stack, minimal, and tiny sizes across every view. The live
    // slot, usage, and link persist so the live agent block, cost rows, and
    // links section are all exercised.
    for (w, h) in [(120u16, 40u16), (80, 24), (50, 14), (30, 8), (24, 6)] {
        for view in View::ORDER {
            state.view = view;
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|f| super::render::draw(f, &state)).unwrap();
        }
    }

    // Overlays.
    for (help, composer, gate) in [(true, false, false), (false, true, false), (false, false, true)]
    {
        let mut s = state_with_plan();
        s.select_task("2");
        s.help = help;
        if composer {
            s.composer = Some(super::state::Composer {
                task: "1".into(),
                slot: Some(0),
                input: "hi".into(),
                kind: super::state::ComposerKind::Intervene,
            });
        }
        s.gate_active = gate;
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| super::render::draw(f, &s)).unwrap();
    }

    // The gate composer in both flavours, so neither hint line — the one that
    // warns a blank result will be refused and the one that does not — goes
    // unrendered. §FS-rhei-run-tui.1.5.5
    for terminal_target in [true, false] {
        let mut s = state_with_plan();
        s.select_task("2");
        s.composer = Some(super::state::Composer {
            task: "2".into(),
            slot: None,
            input: String::new(),
            kind: super::state::ComposerKind::GateResult {
                from: "human-gate".into(),
                to: "completed".into(),
                terminal: terminal_target,
            },
        });
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| super::render::draw(f, &s)).unwrap();
    }
}

/// A verify → needs-human machine where only `verify` declares artifacts, and
/// one task parked in `needs-human` after leaving `verify`. §FS-rhei-viz.4
fn parked_model() -> VizModel {
    let verify = MachineState {
        outputs: vec![crate::rhei_viz_model::Artifact {
            name: "verification".into(),
            path: "runtime/{task_id}.verification.md".into(),
            description: None,
            optional: false,
        }],
        ..machine_state("verify", false, vec!["needs-human"])
    };
    VizModel {
        plan_title: Some("Parked".into()),
        plan_state: Some("active".into()),
        about: None,
        tasks: vec![TaskRow {
            id: "1".into(),
            title: "Alpha".into(),
            parent: None,
            depth: 0,
            state: "needs-human".into(),
            visit_count: None,
            prior: vec![],
            history: vec![crate::rhei_viz_model::StateHistoryEntry {
                from: "verify".into(),
                to: "needs-human".into(),
            }],
        }],
        machine: Machine {
            name: "ci".into(),
            states: vec![verify, machine_state("needs-human", true, vec![])],
        },
    }
}

#[test]
fn artifacts_borrow_previous_state_outputs_when_state_declares_none() {
    let mut state = UiState::with_context(PathBuf::from("/ws"), 2, 2, None, None, None, false);
    state.plan = parked_model();
    state.refresh_plan();
    let task = state.task("1").unwrap().clone();

    let rows = super::derive::artifact_rows(&state, &task);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].input);
    assert_eq!(rows[0].name, "verification");
    assert_eq!(rows[0].from_state.as_deref(), Some("verify"));

    let sections = super::derive::inspector_sections(&state, "1");
    let artifacts = sections
        .iter()
        .find(|s| s.kind == super::derive::InspectorSectionKind::Artifacts)
        .expect("a parked state borrows the previous state's outputs");
    assert_eq!(artifacts.items.len(), 1);
    assert_eq!(artifacts.items[0].label, "out ▸ verification");
}

#[test]
fn artifacts_prefer_the_states_own_contracts() {
    let mut plan = parked_model();
    plan.tasks[0].state = "verify".into();
    let mut state = UiState::with_context(PathBuf::from("/ws"), 2, 2, None, None, None, false);
    state.plan = plan;
    state.refresh_plan();
    let task = state.task("1").unwrap().clone();

    let rows = super::derive::artifact_rows(&state, &task);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].from_state, None);
}

#[test]
fn inspector_excerpts_existing_artifact_content() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("runtime")).unwrap();
    std::fs::write(
        workspace.path().join("runtime/1.verification.md"),
        "NEEDS-HUMAN: what is this project's check command?\n\nNothing here looked like one.\n",
    )
    .unwrap();

    let mut state =
        UiState::with_context(workspace.path().to_path_buf(), 2, 2, None, None, None, false);
    state.plan = parked_model();
    state.refresh_plan();
    let task = state.task("1").unwrap().clone();

    let (lines, _) = super::views::inspector_lines(&state, &task, false);
    let text: Vec<String> = lines
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.clone()).collect::<String>())
        .collect();
    assert!(
        text.iter()
            .any(|l| l.contains("out ▸ verification runtime/1.verification.md (from verify)")),
        "artifact row should show the resolved path and its source state: {text:?}"
    );
    assert!(
        text.iter().any(|l| l.contains("NEEDS-HUMAN: what is this project's check command?")),
        "artifact excerpt should surface the report content: {text:?}"
    );
}

#[test]
fn sanitizes_control_sequences_for_display() {
    assert_eq!(sanitize_terminal_text("\u{1b}[31mred\u{1b}[0m"), "red");
    assert_eq!(sanitize_terminal_text("a\u{7}b"), "ab");
}

#[test]
fn truncates_with_ellipsis() {
    assert_eq!(truncate_chars("abcdef", 4), "abc…");
    assert_eq!(truncate_chars("abc", 4), "abc");
}

/// `(task, from, to, result)` for one submitted gate decision.
type GateCall = (String, String, String, Option<String>);

/// A stub gate sink recording exactly what the TUI submits.
struct RecordingGate {
    calls: Mutex<Vec<GateCall>>,
    fail_reason: Option<String>,
}

impl crate::rhei_tui::dashboard::GateTransitionSink for RecordingGate {
    fn transition_gate(
        &self,
        task_id: &str,
        from: &str,
        to: &str,
        result: Option<&str>,
    ) -> Result<String, String> {
        self.calls.lock().unwrap().push((
            task_id.to_string(),
            from.to_string(),
            to.to_string(),
            result.map(str::to_string),
        ));
        match &self.fail_reason {
            Some(reason) => Err(reason.clone()),
            None => Ok(to.to_string()),
        }
    }
}

fn state_with_gate(gate: Arc<RecordingGate>) -> UiState {
    let mut state = UiState::with_context(
        PathBuf::from("/ws"),
        2,
        2,
        None,
        None,
        Some(gate as Arc<dyn crate::rhei_tui::dashboard::GateTransitionSink>),
        false,
    );
    state.plan = demo_model();
    state.refresh_plan();
    state.select_task("2");
    state
}

/// Picking a gate target opens the composer for the result message, and Enter
/// submits the choice carrying whatever was typed. Without it the everyday
/// `agent → human-gate → completed` shape could never be finished from the TUI:
/// the server refuses a terminal release with nothing recorded.
// §FS-rhei-run-tui.1.5.5 §FS-rhei-states.3.3
#[test]
fn gate_choice_collects_a_result_before_submitting() {
    let gate = Arc::new(RecordingGate { calls: Mutex::new(Vec::new()), fail_reason: None });
    let mut state = state_with_gate(gate.clone());

    press(&mut state, KeyCode::Enter); // open the chooser
    assert!(state.gate_active);
    press(&mut state, KeyCode::Char('1')); // first explicit target
    assert!(!state.gate_active, "the chooser closes once a target is picked");
    let composer = state.composer.as_ref().expect("the choice opens the result composer");
    match &composer.kind {
        super::state::ComposerKind::GateResult { from, to, terminal } => {
            assert_eq!(from, "human-review");
            assert_eq!(to, "completed");
            assert!(*terminal, "the composer knows a blank line will be refused here");
        }
        _ => panic!("expected a gate composer"),
    }

    for ch in "Reviewed.".chars() {
        press(&mut state, KeyCode::Char(ch));
    }
    press(&mut state, KeyCode::Enter);
    assert!(state.composer.is_none(), "submitting closes the composer");
    assert_eq!(
        gate.calls.lock().unwrap().as_slice(),
        &[(
            "2".to_string(),
            "human-review".to_string(),
            "completed".to_string(),
            Some("Reviewed.".to_string())
        )]
    );
}

/// Enter on an empty composer submits with no message — the server decides
/// whether this edge needs one, and its refusal is echoed in the journal like
/// any other. Esc cancels the move entirely.
// §FS-rhei-run-tui.1.5.5
#[test]
fn an_empty_gate_result_submits_with_no_message_and_esc_cancels() {
    let gate = Arc::new(RecordingGate {
        calls: Mutex::new(Vec::new()),
        fail_reason: Some(
            "Task 2 cannot enter terminal state 'completed' without a result.".into(),
        ),
    });
    let mut state = state_with_gate(gate.clone());

    press(&mut state, KeyCode::Enter);
    press(&mut state, KeyCode::Char('1'));
    press(&mut state, KeyCode::Enter);
    assert_eq!(gate.calls.lock().unwrap().as_slice()[0].3, None, "blank sends no message");
    assert!(
        state.journal.iter().any(|entry| entry.text.contains("without a result")),
        "the refusal reason reaches the journal"
    );

    // Esc from the composer abandons the move: nothing more is submitted.
    press(&mut state, KeyCode::Enter);
    press(&mut state, KeyCode::Char('1'));
    assert!(state.composer.is_some());
    press(&mut state, KeyCode::Esc);
    assert!(state.composer.is_none());
    assert_eq!(gate.calls.lock().unwrap().len(), 1, "Esc submits nothing");
}

/// Once the render thread has restored the terminal there is no journal pane
/// left, and the channel still accepts sends — so a warning routed to it would
/// simply vanish. That is where the run's shutdown notice was going.
// §FS-rhei-run-tui.1.8 §FS-rhei-run.3.2
#[test]
fn warnings_leave_the_journal_for_stderr_once_the_screen_is_restored() {
    let warn = RunEvent::Message {
        level: MessageLevel::Warn,
        text: "Interrupted — terminating 1 invocation(s)".to_string(),
    };
    let error = RunEvent::Message { level: MessageLevel::Error, text: "agent failed".to_string() };
    let info = RunEvent::Message { level: MessageLevel::Info, text: "spawning".to_string() };

    // While the screen is live the journal is the right place for all of them.
    assert!(!message_goes_to_stderr(false, &warn));
    assert!(!message_goes_to_stderr(false, &error));

    assert!(message_goes_to_stderr(true, &warn));
    assert!(message_goes_to_stderr(true, &error));

    // Info is journal chrome with nowhere useful to go on a bare terminal, and
    // non-message events are UI state rather than text.
    assert!(!message_goes_to_stderr(true, &info));
    assert!(!message_goes_to_stderr(true, &RunEvent::PassStarted { pass: 1, ready: Vec::new() }));
}

/// The finished screen is a courtesy to an operator who is still there. An
/// interrupted run has one waiting on a shell prompt instead, and a terminal
/// that reports a failed poll has none at all — parking on either left the
/// engine blocked in its own shutdown, or spun a redraw loop on a dead pty.
// §FS-rhei-run-tui.1.5.7
#[test]
fn the_finished_screen_is_left_when_nobody_is_there_to_quit_it() {
    let key_waiting: std::io::Result<bool> = Ok(true);
    let nothing_pressed: std::io::Result<bool> = Ok(false);
    let terminal_gone: std::io::Result<bool> =
        Err(std::io::Error::new(std::io::ErrorKind::Other, "input/output error"));

    // A run that ended on its own terms stays navigable until `q`.
    assert!(!leave_finished_screen(false, &key_waiting));
    assert!(!leave_finished_screen(false, &nothing_pressed));

    // An interrupted run leaves at once, whatever the input says.
    assert!(leave_finished_screen(true, &key_waiting));
    assert!(leave_finished_screen(true, &nothing_pressed));

    // So does a terminal that has gone away, interrupted or not.
    assert!(leave_finished_screen(false, &terminal_gone));
    assert!(leave_finished_screen(true, &terminal_gone));
}
