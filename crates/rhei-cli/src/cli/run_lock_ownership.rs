// Portable run identity on a held lock, plus Linux-only proof that the
// recorded process owns a displaced run-lock inode.
// §FS-rhei-run-headless.3 §FS-rhei-summary.5

#[cfg(target_os = "linux")]
enum ProcessProbe {
    OwnsLock,
    DoesNotOwnLock,
    Gone,
    Unknown(String),
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RunLockOwner {
    version: u8,
    id: String,
    pid: u32,
    workspace: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    process_start_ticks: Option<u64>,
}

#[cfg(target_os = "linux")]
const RUN_LOCK_OWNER_MAX_BYTES: u64 = 16 * 1024;

/// Record exact run ownership on every platform. Linux adds enough stable
/// identity for a later `/proc` inspection to distinguish pid reuse.
/// §FS-rhei-run-headless.3 §FS-rhei-summary.5
fn write_run_lock_owner(lock: &mut HeldRunLock, id: &str, pid: u32) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let stat_path = PathBuf::from(format!("/proc/{pid}/stat"));
    #[cfg(target_os = "linux")]
    let stat = fs::read_to_string(&stat_path)
        .map_err(|err| format!("{} could not be read: {err}", stat_path.display()))?;
    #[cfg(target_os = "linux")]
    let process_start_ticks = parse_linux_process_start_ticks(&stat)?;
    #[cfg(not(target_os = "linux"))]
    let process_start_ticks = None;
    #[cfg(target_os = "linux")]
    let process_start_ticks = Some(process_start_ticks);
    let owner = RunLockOwner {
        version: 1,
        id: id.to_string(),
        pid,
        workspace: lock.workspace.clone(),
        process_start_ticks,
    };
    let mut body = serde_json::to_vec(&owner)
        .map_err(|err| format!("run-lock ownership could not be serialized: {err}"))?;
    body.push(b'\n');
    lock.file.rewind().map_err(|err| format!("run lock could not be rewound: {err}"))?;
    lock.file.set_len(0).map_err(|err| format!("run lock could not be cleared: {err}"))?;
    lock.file
        .write_all(&body)
        .and_then(|()| lock.file.flush())
        .map_err(|err| format!("run-lock ownership could not be written: {err}"))
}

/// Prove that the descriptor's exact Linux process identity owns a locked file
/// descriptor carrying this run's lock record. Merely finding the pid is not
/// evidence: it may now belong to an unrelated process.
// §FS-rhei-run-headless.3
#[cfg(target_os = "linux")]
fn probe_recorded_lock_owner(descriptor: &RunDescriptor) -> ProcessProbe {
    probe_recorded_lock_owner_with(descriptor, |_fd, path| {
        fs::File::open(path).map_err(|err| err.to_string())
    })
}

/// The pre-signal variant can duplicate a target descriptor through the pidfd
/// when the inode itself became unreadable. Positional reads below leave the
/// target process's shared file offset untouched. §FS-rhei-run-headless.7
#[cfg(target_os = "linux")]
fn probe_recorded_lock_owner_for_signal(
    descriptor: &RunDescriptor,
    process: &rustix::fd::OwnedFd,
) -> ProcessProbe {
    use rustix::process::{pidfd_getfd, PidfdGetfdFlags};

    probe_recorded_lock_owner_with(descriptor, |fd, path| match fs::File::open(path) {
        Ok(file) => Ok(file),
        Err(open_err) => pidfd_getfd(process, fd, PidfdGetfdFlags::empty())
            .map(fs::File::from)
            .map_err(|duplicate_err| {
                format!("{open_err}; the held descriptor could not be duplicated: {duplicate_err}")
            }),
    })
}

#[cfg(target_os = "linux")]
fn probe_recorded_lock_owner_with(
    descriptor: &RunDescriptor,
    mut open_locked_file: impl FnMut(i32, &Path) -> Result<fs::File, String>,
) -> ProcessProbe {
    if descriptor.pid == 0 {
        return ProcessProbe::Unknown("recorded pid 0 does not identify one process".to_string());
    }
    let proc_root = PathBuf::from(format!("/proc/{}", descriptor.pid));
    let stat_path = proc_root.join("stat");
    let stat = match fs::read_to_string(&stat_path) {
        Ok(stat) => stat,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return ProcessProbe::Gone,
        Err(err) => {
            return ProcessProbe::Unknown(format!(
                "{} could not be read: {err}",
                stat_path.display()
            ));
        }
    };
    let process_start_ticks = match parse_linux_process_start_ticks(&stat) {
        Ok(start) => start,
        Err(reason) => return ProcessProbe::Unknown(reason),
    };
    let fdinfo_dir = proc_root.join("fdinfo");
    let entries = match fs::read_dir(&fdinfo_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return ProcessProbe::Gone,
        Err(err) => {
            return ProcessProbe::Unknown(format!(
                "{} could not be read: {err}",
                fdinfo_dir.display()
            ));
        }
    };

    let lock_path = descriptor.workspace.join(".rhei/run.lock");
    let mut inconclusive = None;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                inconclusive = Some(format!("{} could not be inspected: {err}", fdinfo_dir.display()));
                continue;
            }
        };
        let fd = entry.file_name();
        let Ok(fd_number) = fd.to_string_lossy().parse::<i32>() else { continue };
        let fdinfo = match fs::read_to_string(entry.path()) {
            Ok(fdinfo) => fdinfo,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                inconclusive = Some(format!("{} could not be read: {err}", entry.path().display()));
                continue;
            }
        };
        if !fdinfo_holds_exclusive_flock(&fdinfo, descriptor.pid) {
            continue;
        }

        let fd_path = proc_root.join("fd").join(&fd);
        let target = match fs::read_link(&fd_path) {
            Ok(target) => target,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                inconclusive = Some(format!("{} could not be inspected: {err}", fd_path.display()));
                continue;
            }
        };
        let metadata = match fs::metadata(&fd_path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                inconclusive = Some(format!("{} could not be inspected: {err}", fd_path.display()));
                continue;
            }
        };
        if !metadata.is_file() {
            continue;
        }
        let file = match open_locked_file(fd_number, &fd_path) {
            Ok(file) => file,
            Err(reason) => {
                inconclusive = Some(format!("{} could not be read: {reason}", fd_path.display()));
                continue;
            }
        };
        let mut body = vec![0; RUN_LOCK_OWNER_MAX_BYTES as usize + 1];
        let bytes = match std::os::unix::fs::FileExt::read_at(&file, &mut body, 0) {
            Ok(bytes) => bytes,
            Err(err) => {
                inconclusive = Some(format!("{} could not be read: {err}", fd_path.display()));
                continue;
            }
        };
        body.truncate(bytes);
        if body.len() as u64 > RUN_LOCK_OWNER_MAX_BYTES {
            if displaced_run_lock_target(&target, &lock_path) {
                inconclusive = Some(format!(
                    "{} is locked by pid {} but its ownership record is too large",
                    target.display(),
                    descriptor.pid
                ));
            }
            continue;
        }
        let owner = match serde_json::from_slice::<RunLockOwner>(&body) {
            Ok(owner) => owner,
            Err(_) if displaced_run_lock_target(&target, &lock_path) => {
                inconclusive = Some(format!(
                    "{} is locked by pid {} but carries no verifiable run ownership",
                    target.display(),
                    descriptor.pid
                ));
                continue;
            }
            Err(_) => continue,
        };
        if owner.version == 1
            && owner.id == descriptor.id
            && owner.pid == descriptor.pid
            && owner.workspace == descriptor.workspace
            && owner.process_start_ticks == Some(process_start_ticks)
        {
            return ProcessProbe::OwnsLock;
        }
    }
    inconclusive.map_or(ProcessProbe::DoesNotOwnLock, ProcessProbe::Unknown)
}

#[cfg(target_os = "linux")]
fn parse_linux_process_start_ticks(stat: &str) -> Result<u64, String> {
    let fields = stat
        .rsplit_once(") ")
        .map(|(_, fields)| fields)
        .ok_or_else(|| "the recorded process stat has no command terminator".to_string())?;
    fields
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| "the recorded process stat has no start identity".to_string())?
        .parse::<u64>()
        .map_err(|err| format!("the recorded process start identity is invalid: {err}"))
}

#[cfg(target_os = "linux")]
fn fdinfo_holds_exclusive_flock(fdinfo: &str, pid: u32) -> bool {
    let pid = pid.to_string();
    fdinfo.lines().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        fields.len() >= 6
            && fields[0] == "lock:"
            && fields[2] == "FLOCK"
            && fields[3] == "ADVISORY"
            && fields[4] == "WRITE"
            && fields[5] == pid
    })
}

#[cfg(target_os = "linux")]
fn displaced_run_lock_target(target: &Path, lock_path: &Path) -> bool {
    let target_text = target.to_string_lossy();
    let lock_text = lock_path.to_string_lossy();
    if target_text == lock_text || target_text == format!("{lock_text} (deleted)") {
        return true;
    }
    target.parent() == lock_path.parent()
        && target
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with("run.lock."))
}
