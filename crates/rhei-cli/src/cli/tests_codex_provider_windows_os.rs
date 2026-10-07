/// Read the actual Windows Zurich rules in a private child using production's
/// selected-zone query/cache and transition conversion. Enumeration changes
/// no host timezone, registry, parent environment or clock. §FS-rhei-run.3.3
#[test]
fn codex_windows_native_os_rules_in_private_child() {
    const CHILD: &str = "CODEX_WINDOWS_RULES_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let test = std::thread::current().name().unwrap().to_string();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", &test, "--nocapture"]).env(CHILD, "1");
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("RHEI_") {
                command.env_remove(name);
            }
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("native Windows edges verified"));
        return;
    }
    use windows_sys::Win32::System::Time::{
        EnumDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION,
    };
    let selected = (0..1024)
        .find_map(|index| {
            let mut selected = DYNAMIC_TIME_ZONE_INFORMATION::default();
            // SAFETY: enumeration writes a valid fully sized output buffer. §FS-rhei-run.3.3
            let result = unsafe { EnumDynamicTimeZoneInformation(index, &mut selected) };
            if result != 0 {
                return None;
            }
            let len = selected.TimeZoneKeyName.iter().position(|c| *c == 0).unwrap();
            let name = String::from_utf16(&selected.TimeZoneKeyName[..len]).unwrap();
            (name == "W. Europe Standard Time").then_some(selected)
        })
        .expect("Windows native timezone database includes Zurich's W. Europe Standard Time");
    let zone = provider_local_time::windows::native::LocalZone::from_selected(selected);
    for (text, expected) in [
        ("Mar 28th, 2032 1:58 AM", Some("2032-03-28T00:59:00Z")),
        ("Mar 28th, 2032 1:59 AM", None),
        ("Mar 28th, 2032 2:00 AM", None),
        ("Mar 28th, 2032 2:58 AM", None),
        ("Mar 28th, 2032 2:59 AM", None),
        ("Mar 28th, 2032 3:00 AM", Some("2032-03-28T01:01:00Z")),
        ("Oct 31st, 2032 1:58 AM", Some("2032-10-30T23:59:00Z")),
        ("Oct 31st, 2032 1:59 AM", None),
        ("Oct 31st, 2032 2:00 AM", None),
        ("Oct 31st, 2032 2:58 AM", None),
        ("Oct 31st, 2032 2:59 AM", None),
        ("Oct 31st, 2032 3:00 AM", Some("2032-10-31T02:01:00Z")),
        ("Dec 31st, 2032 11:59 PM", Some("2032-12-31T23:00:00Z")),
    ] {
        let limit = classify(text, observed(), &|minute| zone.resolve(minute));
        assert_eq!(limit.as_ref().map(|limit| limit.next_attempt_at.as_str()), expected, "{text}");
    }
    println!("native Windows edges verified");
}
