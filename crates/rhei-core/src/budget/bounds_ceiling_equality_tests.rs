//! Remedy ownership at equality is independent of numerical clamp provenance.
//! §FS-rhei-budgets.8 §FS-rhei-budgets.2.3

use std::path::PathBuf;

use super::*;

#[test]
#[ignore = "red until #592: equal requests must name the active ceiling without a clamp; implement removes this"]
fn ceiling_equality_shared_bound_remedies_preserve_request_provenance() {
    let files = SettingsFiles {
        machine: Some(PathBuf::from("machine/settings.json")),
        project: Some(PathBuf::from("project/settings.json")),
    };
    let mut failures = Vec::new();
    for key in
        ["transition_limit", "invocations_per_day", "invocation_lifetime_max", "spend_per_day"]
    {
        // An absent machine setting still leaves the built-in as its ceiling.
        for machine in [Some(4), None] {
            let mut sources = vec![BoundSource::Project];
            if key == "transition_limit" {
                sources.push(BoundSource::Plan);
            }
            for source in sources {
                for value in [3, 4, 100] {
                    let request = Some((value, source));
                    let bound = if key == "spend_per_day" {
                        Bound::resolve_money(key, 4, machine, request)
                    } else {
                        Bound::resolve(key, 4, machine, request)
                    }
                    .with_files(files.clone());
                    check(bound, value, source, &mut failures);
                }
            }
        }
    }
    for key in ["transition_limit", "invocations_per_day"] {
        for value in [3, 4, 100] {
            let bound = Bound::resolve_delegated(key, 4, Some((value, BoundSource::Plan)))
                .with_files(files.clone());
            check(bound, value, BoundSource::Plan, &mut failures);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn check(bound: Bound, value: u64, source: BoundSource, failures: &mut Vec<String>) {
    assert_eq!(bound.effective, value.min(4));
    assert_eq!(bound.source, source);
    assert_eq!(bound.requested, (value > 4).then_some(value));
    if value <= 4 && bound.unit == BoundUnit::Count {
        assert_eq!(bound.value_phrase(), format!("{value} ({})", source.as_str()));
    }
    let expected = if value >= 4 {
        format!("set `defaults.{}` in {}", bound.key, bound.files.name(bound.limiter))
    } else if source == BoundSource::Project {
        format!("set `defaults.{}` in {}", bound.key, bound.files.name(Limiter::Project))
    } else {
        format!("set `{}` on the node's profile in the state machine", bound.key)
    };
    if bound.remedy() != expected {
        failures.push(format!(
            "{} {source:?} request {value}, {:?} ceiling: expected {expected:?}, got {:?}",
            bound.key,
            bound.limiter,
            bound.remedy()
        ));
    }
}
