// The name of a test's own directory, and the tests that hold it to its rule.
//
// Shared verbatim by the CLI's two harnesses — one pulls it in with `#[path]`,
// the other with `include!` into its flat module — so the comments here are
// `//` rather than `//!`: an inner doc comment cannot open an included file.

/// Name a directory that belongs to one test and to nothing else.
///
/// `stem` is what a reader should see in the path; everything after it is the
/// discriminator, whose only job is to be different every time. A test's
/// directory is its own by construction rather than by luck
/// §REQ-cross-platform.6, because nothing downstream catches two tests that
/// were handed one name: creating a directory that is already there succeeds,
/// and the two then take each other's fixtures apart.
///
/// This is where the three ingredients are *read*: the clock, this process, and
/// how many names this process has already asked for. The rule that combines
/// them is [`name_from`], which reads nothing.
pub fn unique_dir_name(stem: &str) -> String {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let clock_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    name_from(stem, clock_nanos, std::process::id(), sequence)
}

/// The naming rule itself, with every ingredient it cannot choose handed to it:
/// what the clock read, which process is asking, and how many names that
/// process has asked for already.
///
/// It is a function of its own so that a test can hold all three still and vary
/// exactly one. Uniqueness that only appears when the clock happens to move is
/// not uniqueness — it is the resolution of whatever clock the platform gave the
/// runner, which is the whole of the difference between a suite that is green
/// where it was written and red where it is gated §REQ-cross-platform.6. A rule
/// that read its own ingredients could not be held that way, and a test of it
/// would pass on whichever ingredient the platform happened to move.
///
/// So the name carries three ingredients and leans on none of them alone
/// §REQ-cross-platform.6, and one test below holds two of them still and varies
/// the third, for each of the three. The sequence counts this process's calls,
/// and is what makes two names distinct when the clock has not moved between
/// them; the process tells two runners apart, because each counts its own calls
/// from zero and `cargo test` runs the harnesses side by side; the clock reading
/// tells this run of a harness from the last one, whose files may still be on
/// disk. This is the rule the CLI already spells for the artifacts it publishes
/// — `invocation_file_id` and `unique_staging_path` in
/// `crates/rhei-cli/src/cli/accounting.rs`.
fn name_from(stem: &str, clock_nanos: u128, process: u32, sequence: u64) -> String {
    format!("{stem}-{clock_nanos}-{process}-{sequence}")
}

/// One clock reading, one process: the sequence is the whole of what is left, so
/// dropping it from the name collapses these names onto one.
///
/// The clock reading a test gets belongs to the runner, not to the test: one
/// platform hands out a distinct one on practically every call and another
/// repeats itself for a whole tick. So the rule is held with the clock frozen.
/// A name that is only distinct because the clock moved is distinct on one
/// platform and not on the next, which is the failure agent-grounds/rhei#340
/// reports — and a test that calls the helper twice in a row cannot see it,
/// because on the finer clock it passes with the defect present.
#[test]
fn dir_names_differ_when_the_clock_stands_still() {
    const CALLS: u64 = 64;
    let frozen = 1_700_000_000_000_000_000u128;

    let names: std::collections::BTreeSet<String> = (0..CALLS)
        .map(|sequence| name_from("rhei-integ-attempt-cross-root", frozen, 4242, sequence))
        .collect();

    assert_eq!(
        names.len() as u64,
        CALLS,
        "{CALLS} directories asked for under one clock reading got {} name(s): {names:?}",
        names.len()
    );
}

/// `cargo test` runs the harnesses as separate processes against one temporary
/// directory, so two of them reading one tick must still be told apart.
///
/// Each harness counts its own calls from zero, so the sequence is frozen here
/// as well as the clock: the process id is the whole of what differs, and
/// dropping it from the name is what this test is here to catch.
#[test]
fn dir_names_differ_between_two_processes_reading_one_tick() {
    let frozen = 1_700_000_000_000_000_000u128;

    assert_ne!(
        name_from("rhei-integ-shared", frozen, 4242, 0),
        name_from("rhei-integ-shared", frozen, 4243, 0),
        "two processes reading one clock tick were handed one directory name"
    );
}

/// A harness that runs twice leaves the first run's directories behind, and the
/// second run counts its calls from zero again under the same process id the
/// operating system is free to hand back. So the clock reading is what tells the
/// two runs apart, and with the other two ingredients frozen it is the whole of
/// what differs.
#[test]
fn dir_names_differ_between_two_runs_of_one_harness() {
    let earlier = 1_700_000_000_000_000_000u128;
    let later = 1_700_000_000_000_000_001u128;

    assert_ne!(
        name_from("rhei-integ-shared", earlier, 4242, 0),
        name_from("rhei-integ-shared", later, 4242, 0),
        "two runs of one harness under one process id were handed one directory name"
    );
}

/// The public helper, not the rule underneath it: whatever it mixes in, calls
/// that follow one another still come back distinct. This is the one test that
/// holds [`unique_dir_name`] to advancing the sequence it reads, which the rule
/// above cannot see.
#[test]
fn dir_names_differ_across_back_to_back_calls() {
    const CALLS: usize = 64;
    let names: std::collections::BTreeSet<String> =
        (0..CALLS).map(|_| unique_dir_name("rhei-integ-back-to-back")).collect();

    assert_eq!(names.len(), CALLS, "back-to-back calls got {} name(s)", names.len());
}

/// The discriminator may be anything; the stem is what a person reads in a
/// failing test's path, so it stays in front of it.
#[test]
fn dir_names_keep_their_stem_readable() {
    let name = unique_dir_name("rhei-integ-attempt-cross-root");

    assert!(
        name.starts_with("rhei-integ-attempt-cross-root-"),
        "the stem should open the name, got {name}"
    );
}
