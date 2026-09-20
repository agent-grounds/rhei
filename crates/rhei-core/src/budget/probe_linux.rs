//! Linux acquisition implementation, not a release qualification. The signed
//! grant pins the image, client, case arguments and bubblewrap bytes.
//! §FS-rhei-budgets.12 §FS-rhei-budgets.6.4

use super::probe::{now_unix, refused, write_new};
use super::{LaunchTuple, NativeProbeBackend, ProbeAuthorization, ProbeCase, Result, RunningProbe};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinuxConfiguration {
    schema: String,
    bubblewrap_sha256: String,
    client: String,
    python: String,
    image: BTreeMap<String, super::probe_image::ImageFile>,
    cases: BTreeMap<String, Vec<String>>,
    // This must name the dedicated, externally capped acquisition broker.
    // Its provider/account routing still requires independent proof.
    broker_socket: PathBuf,
}

/// Created only from an independently authorized grant. Running this backend
/// cannot create a QualifiedLaunch or install registry entries.
/// §FS-rhei-budgets.12
pub struct LinuxProbeBackend {
    tuple: LaunchTuple,
    config: LinuxConfiguration,
    config_bytes: Vec<u8>,
    image_source: PathBuf,
    bubblewrap: PathBuf,
    remaining_launches: u64,
    expires: u64,
    maximum_bytes: u64,
    grant_hash: String,
}

impl LinuxProbeBackend {
    pub fn new(
        authorization: &ProbeAuthorization,
        configuration: &[u8],
        image_source: PathBuf,
        bubblewrap: PathBuf,
    ) -> Result<Self> {
        let grant = &authorization.grant;
        let hash = super::journal::digest(configuration);
        if grant.tuple.os != "linux"
            || grant.tuple.architecture != std::env::consts::ARCH
            || !grant.artifacts.iter().any(|a| {
                a.role == super::EvidenceRole::LaunchConfiguration
                    && a.sha256 == hash
                    && a.bytes == configuration.len() as u64
            })
        {
            return Err(refused("native configuration is not owned by this signed grant"));
        }
        let config: LinuxConfiguration = serde_json::from_slice(configuration)?;
        if config.schema != "rhei.probe.linux.v1"
            || config.image.get(&config.client).map(|f| f.sha256.as_str())
                != Some(grant.tuple.executable_sha256.as_str())
            || !config.image.get(&config.client).is_some_and(|f| f.executable)
            || !config.image.get(&config.python).is_some_and(|f| f.executable)
        {
            return Err(refused(
                "native image lacks the exact client or pinned supervisor runtime",
            ));
        }
        Ok(Self {
            tuple: grant.tuple.clone(),
            config,
            config_bytes: configuration.to_vec(),
            image_source,
            bubblewrap,
            remaining_launches: grant.maximum_launches,
            expires: grant.expires_unix,
            maximum_bytes: grant.maximum_artifact_bytes,
            grant_hash: super::journal::digest(&authorization.evidence),
        })
    }
}

impl NativeProbeBackend for LinuxProbeBackend {
    fn tuple(&self) -> &LaunchTuple {
        &self.tuple
    }

    fn start(
        &mut self,
        case: ProbeCase,
        directory: &Path,
        deadline: Instant,
        permit: super::ProbeLaunchPermit,
    ) -> Result<Box<dyn RunningProbe>> {
        if permit.grant_hash != self.grant_hash
            || permit.deadline != deadline
            || serde_json::to_value(permit.case)? != serde_json::to_value(case)?
        {
            return Err(refused("native launch permit does not match this case and grant"));
        }
        if self.remaining_launches == 0 || now_unix()? >= self.expires || Instant::now() >= deadline
        {
            return Err(refused("native probe launch allowance or deadline exhausted"));
        }
        self.remaining_launches -= 1;
        let name = serde_json::to_value(case)?.as_str().expect("case name").to_owned();
        let argv = self
            .config
            .cases
            .get(&name)
            .filter(|args| !args.is_empty() && args[0].starts_with('/'))
            .ok_or_else(|| refused("signed native configuration lacks this probe case"))?;
        let directory = std::fs::canonicalize(directory)?;
        let image = directory.join("image");
        super::probe_image::copy_image(
            &self.image_source,
            &image,
            &self.config.image,
            self.maximum_bytes,
        )?;
        for name in ["proc", "dev", "tmp", "work", "rhei"] {
            std::fs::create_dir_all(image.join(name))?;
        }
        for name in ["launch.json", "supervisor.py", "broker.sock"] {
            write_new(&image.join("rhei").join(name), &[])?;
        }
        let bytes = super::probe_image::read_file(&self.bubblewrap, self.maximum_bytes)?;
        if super::journal::digest(&bytes) != self.config.bubblewrap_sha256 {
            return Err(refused("bubblewrap executable differs from signed launch configuration"));
        }
        let executable = directory.join("bubblewrap");
        write_new(&executable, &bytes)?;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o500))?;
        if !std::fs::symlink_metadata(&self.config.broker_socket)?.file_type().is_socket() {
            return Err(refused("dedicated acquisition broker socket is unavailable"));
        }
        let launch = directory.join("launch.json");
        write_new(&launch, &serde_json::to_vec(argv)?)?;
        let supervisor = directory.join("supervisor.py");
        write_new(&supervisor, include_bytes!("probe_supervisor.py"))?;
        let config = directory.join("configuration.json");
        write_new(&config, &self.config_bytes)?;
        let mut artifacts = vec![
            config,
            supervisor.clone(),
            launch.clone(),
            executable.clone(),
            directory.join("launch-intent.json"),
        ];
        artifacts.extend(self.config.image.keys().map(|name| image.join(name)));
        let mut prepared_bytes = 0u64;
        for path in &artifacts {
            prepared_bytes = prepared_bytes
                .checked_add(std::fs::metadata(path)?.len())
                .ok_or_else(|| refused("native evidence size overflow"))?;
        }
        let output_limit = self
            .maximum_bytes
            .checked_sub(prepared_bytes)
            .filter(|remaining| *remaining >= 2)
            .ok_or_else(|| refused("prepared native evidence leaves no capture allowance"))?
            / 2;
        let mut cmd = Command::new(executable);
        cmd.env_clear()
            .args([
                "--unshare-user",
                "--unshare-pid",
                "--unshare-net",
                "--unshare-ipc",
                "--unshare-uts",
                "--disable-userns",
                "--die-with-parent",
                "--new-session",
                "--as-pid-1",
                "--cap-drop",
                "ALL",
                "--clearenv",
                "--ro-bind",
            ])
            .arg(image)
            .arg("/")
            .args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--tmpfs", "/work"])
            .arg("--ro-bind")
            .arg(&launch)
            .arg("/rhei/launch.json")
            .arg("--ro-bind")
            .arg(&supervisor)
            .arg("/rhei/supervisor.py")
            .arg("--ro-bind")
            .arg(&self.config.broker_socket)
            .arg("/rhei/broker.sock")
            .args([
                "--chdir",
                "/work",
                "--setenv",
                "HOME",
                "/work",
                "--setenv",
                "PATH",
                "/usr/bin:/bin",
                "--",
            ])
            .arg(format!("/{}", self.config.python))
            .args(["-I", "-S", "/rhei/supervisor.py"])
            .arg(deadline.saturating_duration_since(Instant::now()).as_millis().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Recheck immediately before spawn; preparation cannot spend the deadline.
        if Instant::now() >= deadline || now_unix()? >= self.expires {
            return Err(refused("native preparation exceeded the probe deadline"));
        }
        let child = cmd.spawn()?;
        let mut running = LinuxRunningProbe {
            child,
            capture: vec![],
            failed: Arc::new(AtomicBool::new(false)),
            artifacts,
            stopped: false,
        };
        let out = running.child.stdout.take().expect("piped stdout");
        let err = running.child.stderr.take().expect("piped stderr");
        for (label, stream) in
            [("stdout", Box::new(out) as Box<dyn Read + Send>), ("stderr", Box::new(err))]
        {
            let path = directory.join(format!("{label}.log"));
            running.capture.push(capture(
                stream,
                path.clone(),
                output_limit,
                running.failed.clone(),
            ));
            running.artifacts.push(path);
        }
        Ok(Box::new(running))
    }
}

struct LinuxRunningProbe {
    child: Child,
    capture: Vec<std::thread::JoinHandle<Result<()>>>,
    failed: Arc<AtomicBool>,
    artifacts: Vec<PathBuf>,
    stopped: bool,
}
impl RunningProbe for LinuxRunningProbe {
    fn poll(&mut self) -> Result<Option<i32>> {
        if self.failed.load(Ordering::Acquire) {
            return Err(refused("native capture failed or exceeded cap"));
        }
        Ok(self.child.try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }
    fn terminate_tree(&mut self) -> Result<()> {
        if !self.stopped {
            if self.child.try_wait()?.is_none() {
                self.child.kill()?;
            }
            self.child.wait()?;
            self.stopped = true;
        }
        let mut failure = None;
        for handle in self.capture.drain(..) {
            if let Err(error) =
                handle.join().unwrap_or_else(|_| Err(refused("native capture panicked")))
            {
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
    fn artifact_paths(&self) -> Vec<PathBuf> {
        self.artifacts.clone()
    }
}
impl Drop for LinuxRunningProbe {
    fn drop(&mut self) {
        let _ = self.terminate_tree();
    }
}
fn capture(
    mut stream: Box<dyn Read + Send>,
    path: PathBuf,
    limit: u64,
    failed: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Result<()>> {
    std::thread::spawn(move || {
        let result = (|| {
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
            let mut bytes = 0u64;
            let mut buffer = [0; 8192];
            loop {
                let read = stream.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                bytes += read as u64;
                if bytes > limit {
                    return Err(refused("native output exceeds acquisition cap"));
                }
                file.write_all(&buffer[..read])?;
            }
            file.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            failed.store(true, Ordering::Release);
        }
        result
    })
}
