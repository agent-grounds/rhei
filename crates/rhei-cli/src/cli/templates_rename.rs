/// Publish a staged directory in one operation without replacing a competing
/// destination. Unsupported kernels/filesystems fail closed; a check followed
/// by a replacing rename would reopen the race. §FS-rhei-templates.6.1.2
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "redox"
))]
fn rename_member_noreplace(staged: &Path, output: &Path) -> std::io::Result<()> {
    use rustix::fs::{renameat_with, RenameFlags, CWD};
    renameat_with(CWD, staged, CWD, output, RenameFlags::NOREPLACE).map_err(Into::into)
}

/// MoveFileExW without MOVEFILE_REPLACE_EXISTING refuses every existing target;
/// same-parent staging keeps the move on one volume. §FS-rhei-templates.6.1.2
#[cfg(windows)]
fn rename_member_noreplace(staged: &Path, output: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    fn wide(path: &Path) -> std::io::Result<Vec<u16>> {
        // Keep the verbatim prefix for long paths without resolving the
        // destination itself: only its existing parent is canonicalized.
        let name = path.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no final name")
        })?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let path = fs::canonicalize(parent)?.join(name);
        let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
        if units.contains(&0) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL"));
        }
        units.push(0);
        Ok(units)
    }
    let staged = wide(staged)?;
    let output = wide(output)?;
    // SAFETY: both buffers are NUL-terminated and remain alive for the call.
    if unsafe { MoveFileExW(staged.as_ptr(), output.as_ptr(), 0) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Never substitute a replacing rename on an unsupported platform.
/// §FS-rhei-templates.6.1.2
#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "redox",
    windows
)))]
fn rename_member_noreplace(_staged: &Path, _output: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-replace member publication is unavailable on this platform",
    ))
}
