/// Repeated source names retain distinct reasons and state selection. §FS-rhei-viz.4
#[test]
fn operator_inspector_preserves_each_repeated_source_reason() {
    use super::derive::{ChipAction, InspectorSectionKind};
    use crate::rhei_viz_model::StateHistoryEntry;
    let mut state = UiState::with_context(PathBuf::from("/ws"), 2, 2, None, None, None, false);
    state.plan = parked_model();
    state.plan.tasks[0].state = "done".into();
    state.plan.tasks[0].history = vec![
        StateHistoryEntry {
            from: "gate".into(),
            to: "work".into(),
            forced_reason: Some("first correction".into()),
        },
        StateHistoryEntry {
            from: "work".into(),
            to: "gate".into(),
            forced_reason: Some("return to gate".into()),
        },
        StateHistoryEntry { from: "gate".into(), to: "done".into(), forced_reason: None },
    ];
    state.refresh_plan();
    let sections = super::derive::inspector_sections(&state, "1");
    let history = sections
        .iter()
        .find(|section| section.kind == InspectorSectionKind::PreviousStates)
        .unwrap();
    assert_eq!(
        history.items.iter().map(|chip| chip.label.as_str()).collect::<Vec<_>>(),
        ["gate", "work — forced: return to gate", "gate — forced: first correction"]
    );
    for (chip, expected) in history.items.iter().zip(["gate", "work", "gate"]) {
        assert!(matches!(&chip.action, ChipAction::MarkState(state) if state == expected));
    }
}
