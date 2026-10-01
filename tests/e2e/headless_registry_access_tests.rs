//! Registry outages leave unrelated exact-id stop available. §FS-rhei-run-headless.3

// Permission failures and detached startup use Unix facilities. §FS-rhei-run-headless.1.3
#![cfg(unix)]

use super::headless_support::{wait_until, Workspace};
use super::{stderr, stdout};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

/// Two live runs share a registry, but only one loses root access. §FS-rhei-recover.4
#[test]
fn headless_unreadable_root_keeps_unknown_and_allows_unrelated_stop_by_id() {
    let blind = Workspace::new("headless-uncheckable", 60);
    let mut healthy = Workspace::new("headless-unrelated", 60);
    healthy.home = blind.home.clone();
    let blind_id = blind.launch_headless();
    let healthy_id = healthy.launch_headless();
    wait_until("both runs to claim a ticket", Duration::from_secs(30), || {
        [&blind, &healthy].into_iter().all(|ws| {
            fs::read_to_string(ws.root.join("runtime/events.jsonl"))
                .is_ok_and(|text| text.contains("slot_assigned"))
        })
    });
    let marker = blind.root.join(".rhei/forced-recovery.json");
    assert!(!marker.exists());
    let registry = blind.home.join("state/rhei/runs").join(format!("{blind_id}.json"));
    let paths = [
        blind.plan(),
        blind.root.join("runtime/run.json"),
        blind.root.join(".rhei/run.lock"),
        registry,
    ];
    let before = paths.each_ref().map(|path| fs::read(path).unwrap());
    let restricted = blind.root.join(".rhei");
    // One outage per claim, each closed before its own snapshot: a window
    // spanning both commands cannot say which left the root alone, and three of
    // these paths need `.rhei` searchable to read. §FS-rhei-run-headless.3
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o000)).unwrap();
    let listing = healthy.rhei(&["runs"]);
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o755)).unwrap();
    let after_listing = paths.each_ref().map(|path| fs::read(path).unwrap());
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o000)).unwrap();
    let stopped = healthy.rhei(&["stop", &healthy_id, "--wait"]);
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o755)).unwrap();
    let after_stop = paths.each_ref().map(|path| fs::read(path).unwrap());
    let readable = blind.rhei(&["runs"]);
    // Tidy both real runs before checking outputs that could fail the test.
    let blind_stop = blind.rhei(&["stop", &blind_id, "--wait"]);
    let _ = healthy.rhei(&["stop", &healthy_id, "--wait"]);

    assert!(listing.status.success(), "{}", stderr(&listing));
    let text = stdout(&listing);
    assert!(text.contains(&blind_id) && text.contains("could not be checked"), "{text}");
    assert!(text.contains(&format!("{healthy_id}  running")), "{text}");
    assert!(!stderr(&listing).contains("forced recovery pending"));
    assert!(stopped.status.success(), "{}", stderr(&stopped));
    assert!(stdout(&stopped).contains(&format!("Asked run {healthy_id}")), "{}", stdout(&stopped));
    // Each outage is answerable on its own: the blind listing neither read nor
    // mutated the root it could not check, and stopping an unrelated run did
    // not reach it either. §FS-rhei-run-headless.3
    assert_eq!(after_listing, before, "the blind listing left the inaccessible root untouched");
    assert_eq!(after_stop, after_listing, "nor did the unrelated stop reach it");
    assert!(!marker.exists());
    // Access is back, so the entry is classified afresh §FS-rhei-run-headless.3 —
    // live here — and a listing with no flags is live runs only, not the undecided
    // block that printed this same line blind. §FS-rhei-run-headless.6.1
    let again = stdout(&readable);
    assert!(!again.contains("could not be checked"), "nothing is undecided now: {again}");
    assert!(again.contains(&format!("{blind_id}  running")), "{again}");
    assert!(blind_stop.status.success(), "{}", stderr(&blind_stop));
}
