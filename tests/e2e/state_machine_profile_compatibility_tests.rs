//! The legacy/profiled compatibility boundary is independent of the rest of
//! the machine's validation. §FS-rhei-states.1.1, §FS-rhei-states.1.3

use super::*;

// The open/done graph from the original #634 reproducer, with only its version,
// initial flag, and two policy blocks varied between cases.
const GRAPH: &str = "states:\n  o: {}\n  d: {final: true}\ntransitions:\n  - {from: o, to: d}\n";
const PROFILES: &str = "profiles:\n  p:\n    initial: o\n    allowed: [o, d]\n";
const POLICY: &str = "node_policy:\n  root: p\n  default: p\n";

fn load(version: &str, initial: bool, profiles: bool, policy: bool) -> CliRun {
    let dir = unique_temp_dir("profile-compatibility");
    let graph = if initial { GRAPH.replace("o: {}", "o: {initial: true}") } else { GRAPH.into() };
    let yaml = format!(
        "name: m\nversion: {version}\n{graph}{}{}",
        if profiles { PROFILES } else { "" },
        if policy { POLICY } else { "" },
    );
    write_fixture_file(&dir, "states.yaml", &yaml);
    let output = rhei_command(dir.join("home"))
        .current_dir(&dir)
        .args(["states", "--state-machine", "states.yaml"])
        .output()
        .expect("states command should run");
    CliRun::from(&output)
}

fn assert_loaded(run: &CliRun, case: &str) {
    assert!(run.status.success(), "{case}: {}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("State machine: m"), "{case}: {}", run.stdout);
}

fn assert_refused(run: &CliRun, case: &str, cause: &str) {
    assert_eq!(run.status.code(), Some(1), "{case}: {}\n{}", run.stdout, run.stderr);
    assert!(run.stderr.contains(cause), "{case}: {}", run.stderr);
}

/// §FS-rhei-states.1.1 permits the legacy v1/v2 machines and the profiled v1
/// control from the original reproducer, while requiring both blocks at v3+.
#[test]
fn legacy_profiles_compatibility_complete_shapes() {
    for version in ["1", "2", "3", "4"] {
        assert_loaded(&load(version, false, true, true), &format!("profiled v{version}"));
        let legacy = load(version, true, false, false);
        if matches!(version, "1" | "2") {
            assert_loaded(&legacy, &format!("legacy v{version}"));
        } else {
            assert_refused(&legacy, &format!("legacy v{version}"), "requires 'profiles' and");
        }
    }
}

/// §FS-rhei-states.1.3 refuses either single-block shape and initial: true
/// alongside profiles at every version, including the compatibility versions.
#[test]
fn legacy_profiles_compatibility_rejects_mixed_shapes() {
    for version in ["1", "2", "3"] {
        for (profiles, policy, cause) in [
            (true, false, "declares 'profiles' but no 'node_policy'"),
            (false, true, "declares 'node_policy' but no 'profiles'"),
            (true, true, "declares 'initial: true'"),
        ] {
            assert_refused(
                &load(version, true, profiles, policy),
                &format!("v{version}, profiles={profiles}, policy={policy}"),
                cause,
            );
        }
    }
}

/// §FS-rhei-states.1.1 describes a compatibility check rather than semver:
/// fractional numbers have no major, while strings use their first component.
#[test]
fn legacy_profiles_compatibility_version_interpretation() {
    for version in ["3.1", "2.9", "-3", "\"v3\"", "\"unknown\"", "\"2.99\""] {
        assert_loaded(&load(version, true, false, false), version);
    }
    for version in ["3.0", "\"3\"", "\"3.1\"", "\"3.anything\"", "\"4.0\""] {
        assert_refused(&load(version, true, false, false), version, "requires 'profiles' and");
    }
}
