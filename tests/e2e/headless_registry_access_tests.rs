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
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o000)).unwrap();
    let listing = healthy.rhei(&["runs"]);
    let stopped = healthy.rhei(&["stop", &healthy_id, "--wait"]);
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o755)).unwrap();
    let after = paths.each_ref().map(|path| fs::read(path).unwrap());
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
    assert_eq!(after, before, "the inaccessible root and its external entry remain untouched");
    assert!(!marker.exists());
    assert!(stdout(&readable).contains(&format!("{blind_id}  running")), "{}", stdout(&readable));
    assert!(blind_stop.status.success(), "{}", stderr(&blind_stop));
}
