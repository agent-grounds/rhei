// Boundary hooks exist only in unit-test binaries, never as CLI authority. §FS-rhei-recover.5
#[cfg(test)]
thread_local! {
    static FORCED_BOUNDARY: std::cell::RefCell<Option<Box<dyn FnMut(&str) -> MietteResult<()>>>> =
        const { std::cell::RefCell::new(None) };
}

fn forced_boundary(_name: &str) -> MietteResult<()> {
    #[cfg(test)]
    FORCED_BOUNDARY.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() { hook(_name) } else { Ok(()) }
    })?;
    Ok(())
}

/// Directory entries are durable before the next transaction phase. §FS-rhei-recover.5
fn forced_sync_directory(path: &Path) -> MietteResult<()> {
    #[cfg(not(windows))]
    fs::File::open(path).and_then(|file| file.sync_all())
        .map_err(|err| file_io_report(path, "failed to sync recovery directory", err))?;
    // Windows uses write-through MoveFileExW for namespace changes instead.
    #[cfg(windows)]
    let _ = path;
    Ok(())
}

/// Sync newly created directory entries all the way to an existing parent. §FS-rhei-recover.2
fn forced_create_directory(path: &Path) -> MietteResult<()> {
    if path.is_dir() { return Ok(()); }
    if let Some(parent) = path.parent() { forced_create_directory(parent)?; }
    fs::create_dir(path).map_err(|err| file_io_report(path, "failed to create recovery directory", err))?;
    if let Some(parent) = path.parent() { forced_sync_directory(parent)?; }
    Ok(())
}

/// Same-directory atomic replacement; the Windows move is write-through. §FS-rhei-recover.5
fn forced_rename(source: &Path, destination: &Path) -> MietteResult<()> {
    #[cfg(not(windows))]
    fs::rename(source, destination).map_err(|err| file_io_report(destination, "failed to replace recovery file", err))?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" { fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32; }
        let source = source.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let target = destination.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        // Both buffers are NUL-terminated and live through the call; 1|8 is
        // MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH. §FS-rhei-recover.5
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 1 | 8) } == 0 {
            return Err(file_io_report(destination, "failed to replace recovery file", std::io::Error::last_os_error()));
        }
    }
    forced_sync_directory(destination.parent().expect("recovery file has parent"))
}

/// Preserve the destination's permissions as well as its complete image. §FS-rhei-recover.2
fn forced_replace(path: &Path, bytes: &[u8]) -> MietteResult<()> {
    let parent = path.parent().ok_or_else(|| miette!("recovery file has no parent"))?;
    forced_create_directory(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| file_io_report(path, "failed to create recovery temporary", err))?;
    if let Ok(metadata) = fs::metadata(path) {
        temporary.as_file().set_permissions(metadata.permissions())
            .map_err(|err| file_io_report(path, "failed to preserve recovery file permissions", err))?;
    }
    temporary.write_all(bytes).and_then(|()| temporary.as_file().sync_all())
        .map_err(|err| file_io_report(path, "failed to sync recovery replacement", err))?;
    let temporary = temporary.into_temp_path();
    forced_rename(&temporary, path)
}

/// A Windows retirement rename makes pathname absence durable before unlink. §FS-rhei-recover.5
fn forced_remove(path: &Path) -> MietteResult<()> {
    if !path.exists() { return Ok(()); }
    #[cfg(windows)]
    {
        let retired = path.with_file_name(format!(".retired-{}", uuid::Uuid::new_v4()));
        forced_rename(path, &retired)?;
        fs::remove_file(&retired).map_err(|err| file_io_report(&retired, "failed to unlink retired recovery file", err))?;
    }
    #[cfg(not(windows))]
    fs::remove_file(path).map_err(|err| file_io_report(path, "failed to remove recovery file", err))?;
    forced_sync_directory(path.parent().expect("recovery file has parent"))
}

/// Install whole images, never append/recompute result or checkpoint effects. §FS-rhei-recover.3
fn forced_install_images(root: &Path, marker: &ForcedMarker, forward: bool) -> MietteResult<()> {
    for (index, file) in marker.files.iter().enumerate() {
        forced_boundary(&format!("image-{index}-before"))?;
        let path = forced_image_path(root, &file.path)?;
        let image = if forward { &file.after } else { &file.before };
        match image.bytes()? {
            Some(bytes) => forced_replace(&path, &bytes)?,
            None => forced_remove(&path)?,
        }
        forced_boundary(&format!("image-{index}-after"))?;
    }
    Ok(())
}

/// The exact synced adjacent pair is the sole commit witness. §FS-rhei-transition-cmd.6.1
fn forced_commit(root: &Path, marker: &ForcedMarker) -> MietteResult<()> {
    let marker_path = root.join(rhei_core::root_access::MARKER);
    forced_boundary("marker-before")?;
    // The run lock may just have created .rhei; persist its parent entry too. §FS-rhei-recover.2
    forced_sync_directory(root)?;
    forced_replace(&marker_path, &marker.bytes()?)?;
    forced_boundary("marker-after")?;
    forced_install_images(root, marker, true)?;
    let path = forced_image_path(root, &marker.ledger.path)?;
    forced_create_directory(path.parent().expect("ledger parent"))?;
    forced_boundary("pair-before")?;
    let mut ledger = fs::OpenOptions::new().create(true).append(true).open(&path)
        .map_err(|err| file_io_report(&path, "failed to open forced ledger", err))?;
    ledger.write_all(marker.ledger.metadata_line.as_bytes())
        .map_err(|err| file_io_report(&path, "failed to write force metadata", err))?;
    forced_boundary("pair-between")?;
    ledger.write_all(marker.ledger.movement_line.as_bytes())
        .map_err(|err| file_io_report(&path, "failed to write force movement", err))?;
    forced_boundary("ledger-sync-before")?;
    ledger.sync_all().map_err(|err| file_io_report(&path, "failed to sync forced ledger", err))?;
    forced_sync_directory(path.parent().expect("ledger parent"))?;
    forced_boundary("ledger-sync-after")?;
    forced_boundary("marker-remove-before")?;
    forced_remove(&marker_path)?;
    forced_boundary("marker-remove-after")
}
