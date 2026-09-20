// Audit identity comes from the OS, never a user-controlled USER variable.
// §FS-rhei-budgets.8

fn budget_audit(reason: &str) -> MietteResult<rhei_core::budget::Audit> {
    if reason.trim().is_empty() {
        return Err(miette!(help = "pass --reason \"<why this allowance changes>\"; the audit record is what makes a money change reviewable", "budget mutation requires a nonempty --reason"));
    }
    Ok(rhei_core::budget::Audit {
        actor: budget_local_actor()?,
        written_at: format_iso8601_utc(std::time::SystemTime::now()),
        reason: reason.into(),
        argv: std::env::args_os().map(|a| a.to_string_lossy().into_owned()).collect(),
    })
}

#[cfg(unix)]
fn budget_local_actor() -> MietteResult<String> {
    extern "C" {
        fn geteuid() -> u32;
    }
    // SAFETY: geteuid has no arguments and returns the process's effective UID.
    Ok(format!("uid:{}", unsafe { geteuid() }))
}

#[cfg(windows)]
fn budget_local_actor() -> MietteResult<String> {
    #[link(name = "advapi32")]
    extern "system" {
        fn GetUserNameW(buffer: *mut u16, size: *mut u32) -> i32;
    }
    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;
    // SAFETY: buffer is writable for size UTF-16 code units, including NUL.
    if unsafe { GetUserNameW(buffer.as_mut_ptr(), &mut size) } == 0 {
        return Err(miette!(
            help = "the audit record names who changed an allowance, so the operating system must answer who that is",
            "cannot authenticate budget operator: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(format!("windows:{}", String::from_utf16_lossy(&buffer[..size.saturating_sub(1) as usize])))
}

#[cfg(not(any(unix, windows)))]
fn budget_local_actor() -> MietteResult<String> {
    Err(miette!(help = "budget mutations run on a platform whose account name rhei can read: Unix or Windows", "this platform cannot authenticate a budget operator"))
}
