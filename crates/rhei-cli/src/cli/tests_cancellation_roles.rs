// General roles and scoped escapes are ordinary runtime semantics.
// §FS-rhei-states.1.4 §FS-rhei-transitions.4.6
mod cancellation_roles {
    use super::*;

    fn machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::parse_fragment(r#"
name: roles
version: 1
states:
  work: {}
  middle: {}
  done: {final: true}
  abandoned: {final: true, role: cancellation}
transitions:
  - {from: work, to: middle, condition: 'visitCount > 9'}
  - {from: middle, to: done}
  - {from: '*', sources: [work], to: abandoned}
"#).unwrap()
    }

    #[test]
    fn scoped_escape_is_neither_completion_nor_conditional_fallback() {
        let machine = machine();
        let plan = rhei_core::parse("# Rhei: T\n\n## Tasks\n\n### Task job: Work\n**State:** work\n").unwrap();
        assert_eq!(find_completion_state("work", &machine), None);
        assert_eq!(find_next_transition(&plan.tasks[0], &plan, &machine).unwrap(), None);
        assert_eq!(find_completion_state("middle", &machine).as_deref(), Some("done"));
        assert!(machine.transition_matches_source(&machine.transitions[2], "work"));
        assert!(!machine.transition_matches_source(&machine.transitions[2], "middle"));
    }

    #[test]
    fn cancellation_roles_control_dependencies_reporting_and_supervision() {
        let mut machine = machine();
        for name in ["abandoned", "cancelled", "canceled"] {
            assert!(!dependency_is_satisfied(name, &machine));
            assert!(!is_successful_completion_state(name, &machine));
            assert!(matches!(classify_marker(name, &machine), Marker::Cancelled));
        }
        assert!(dependency_is_satisfied("done", &machine));
        assert!(matches!(classify_marker("done", &machine), Marker::Done));
        machine.transitions[0].condition = Some("openDescendants == 0".into());
        machine.transitions[0].to.0 = "abandoned".into();
        assert!(!rhei_validator::supervising_state_can_finish(&machine, "work"));
        machine.transitions[0].to.0 = "done".into();
        assert!(rhei_validator::supervising_state_can_finish(&machine, "work"));
    }
}
