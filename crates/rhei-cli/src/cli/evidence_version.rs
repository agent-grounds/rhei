// The agent CLI version an invocation ran with: `version_args` appended to the
// profile's whole command, probed once per run per command. §FS-rhei-evidence.7

/// How long a version probe may take. §FS-rhei-evidence.7
const VERSION_PROBE_BOUND: Duration = Duration::from_secs(10);

/// One probe's answer, and the executable it was taken from.
#[derive(Clone, Debug)]
struct VersionProbe {
    stamp: Option<(u64, Option<std::time::SystemTime>)>,
    version: Option<String>,
    note: Option<String>,
}

/// The command a probe runs: the whole `command`, then `version_args`, never
/// the first element alone. §FS-rhei-evidence.7
fn version_probe_argv(command: &[String], version_args: &[String]) -> Vec<String> {
    command.iter().chain(version_args).cloned().collect()
}

/// The first non-empty line of stdout, trimmed. §FS-rhei-evidence.7
fn version_probe_line(stdout: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Where `program` resolves: itself when it names a path, else the first
/// match on `PATH`.
fn resolved_executable(program: &str) -> Option<PathBuf> {
    let named = Path::new(program);
    if named.components().count() > 1 || named.is_absolute() {
        return Some(named.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        [program.to_string(), format!("{program}.exe")]
            .into_iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// The size and modification time a cached answer is valid for.
fn executable_stamp(program: &str) -> Option<(u64, Option<std::time::SystemTime>)> {
    let metadata = fs::metadata(resolved_executable(program)?).ok()?;
    Some((metadata.len(), metadata.modified().ok()))
}

/// Run one probe, stdin closed, bounded. No model is called. §FS-rhei-evidence.7
fn run_version_probe(argv: &[String], cwd: &Path) -> (Option<String>, Option<String>) {
    use std::io::Read as _;
    let mut command = std::process::Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return (None, Some("probe exited -1".to_string()));
    };
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(out) = stdout.as_mut() {
            let _ = out.read_to_end(&mut bytes);
        }
        bytes
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < VERSION_PROBE_BOUND => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let Some(status) = status else {
        return (None, Some("probe timed out".to_string()));
    };
    let bytes = reader.join().unwrap_or_default();
    if !status.success() {
        return (None, Some(format!("probe exited {}", status.code().unwrap_or(-1))));
    }
    match version_probe_line(&bytes) {
        Some(line) => (Some(line), None),
        None => (None, Some("probe printed nothing".to_string())),
    }
}

impl EvidenceRun {
    /// `(agent_cli_version, agent_cli_version_note)` for `resolved`, from the
    /// cache unless the executable's size or modification time changed. The
    /// command is the profile's as configured: launch adapters append path
    /// flags to the resolved one. §FS-rhei-evidence.7
    fn agent_cli_version(&self, resolved: &ResolvedAgent) -> (Option<String>, Option<String>) {
        let Some(version_args) = resolved.profile.version_args.as_ref() else {
            return (None, Some("no version_args".to_string()));
        };
        let command = self
            .settings
            .agents
            .get(resolved.agent.id())
            .map(|profile| &profile.command)
            .filter(|command| !command.is_empty())
            .unwrap_or(&resolved.profile.command);
        if command.is_empty() {
            return (None, Some("no version_args".to_string()));
        }
        let argv = version_probe_argv(command, version_args);
        let stamp = executable_stamp(&command[0]);
        self.cached_version_probe(argv, stamp, |argv| run_version_probe(argv, &self.root))
    }

    /// The cached answer for `argv` while its executable's `stamp` holds,
    /// else a fresh `probe`. §FS-rhei-evidence.7
    fn cached_version_probe(
        &self,
        argv: Vec<String>,
        stamp: Option<(u64, Option<std::time::SystemTime>)>,
        probe: impl FnOnce(&[String]) -> (Option<String>, Option<String>),
    ) -> (Option<String>, Option<String>) {
        let mut probes = self.probes.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(cached) = probes.get(&argv).filter(|cached| cached.stamp == stamp) {
            return (cached.version.clone(), cached.note.clone());
        }
        let (version, note) = probe(&argv);
        probes.insert(argv, VersionProbe { stamp, version: version.clone(), note: note.clone() });
        (version, note)
    }
}
