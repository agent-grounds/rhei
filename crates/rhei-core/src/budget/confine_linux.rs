//! Linux credential, egress and complete-process-tree confinement for one
//! admitted launch. Building this capability is not a qualification: a tuple
//! still enters the registry only through independently reviewed evidence.
//! §FS-rhei-budgets.6.4 §AR-neural-admission.5

use super::{BudgetError, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Where the admitted invocation's only channel out of the namespace appears.
/// Nothing else crosses the boundary, so a client that cannot use this socket
/// has no provider reach at all. §FS-rhei-budgets.6.4
pub const BROKER_MOUNT: &str = "/rhei/broker.sock";

/// One confined launch: the exact executable, the paths it may read, the one
/// directory it may write, and the exact environment it starts with. Nothing
/// of the operator's session is inherited. §AR-neural-admission.5
pub struct ConfinedLaunch {
    /// Absolute host path of the pinned executable. It is bound read-only at
    /// the same path so the child's argv0 and the qualified digest agree.
    pub program: PathBuf,
    pub argv: Vec<String>,
    /// Absolute host paths bound read-only at the same path. A credential
    /// directory that is not listed here is unreachable, not merely unset.
    pub read_only: Vec<PathBuf>,
    /// The single writable directory, bound at the same path and used as the
    /// working directory and `HOME`.
    pub work: PathBuf,
    /// Host Unix socket relayed at [`BROKER_MOUNT`], or `None` for a launch
    /// with no provider reach whatsoever.
    pub broker_socket: Option<PathBuf>,
    /// The complete environment. `--clearenv` drops everything else, so an
    /// inherited provider key cannot survive into the child.
    pub environment: BTreeMap<String, String>,
}

/// A resolved host that can confine a launch: the `bubblewrap` executable and
/// a demonstrated unprivileged user namespace. §REQ-cross-platform.3
pub struct LinuxConfinement {
    bubblewrap: PathBuf,
}

pub(super) fn missing(message: impl Into<String>) -> BudgetError {
    BudgetError::new("missing_qualification", message)
}

fn find_on_path(name: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH")
        .ok_or_else(|| missing(format!("PATH is unavailable to resolve {name}")))?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return candidate.canonicalize().map_err(BudgetError::from);
        }
    }
    Err(missing(format!("no {name} executable is available to confine a launch")))
}

impl LinuxConfinement {
    /// Resolve the host once. A host without bubblewrap or without usable
    /// unprivileged user namespaces has no confinement, so admission refuses
    /// rather than launching unconfined. §FS-rhei-budgets.6.4
    pub fn resolve() -> Result<Self> {
        let confinement = Self { bubblewrap: find_on_path("bwrap")? };
        confinement.check_namespaces()?;
        Ok(confinement)
    }

    /// The namespaces are demonstrated, not assumed: a kernel that refuses an
    /// unprivileged user namespace must refuse the tuple rather than run
    /// beside it. The probe needs one trivially exiting host executable; with
    /// none available the launch itself reports instead of this check.
    fn check_namespaces(&self) -> Result<()> {
        let Some(probe) = ["/bin/true", "/usr/bin/true"]
            .into_iter()
            .map(PathBuf::from)
            .find(|candidate| candidate.is_file())
        else {
            return Ok(());
        };
        let status = Command::new(&self.bubblewrap)
            .args(["--unshare-user", "--unshare-net", "--unshare-pid", "--unshare-ipc"])
            .args(["--unshare-uts", "--disable-userns", "--cap-drop", "ALL", "--clearenv"])
            .args(["--ro-bind", "/", "/", "--proc", "/proc", "--"])
            .arg(probe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| missing(format!("bubblewrap could not be started: {error}")))?;
        if !status.success() {
            return Err(missing(
                "this kernel refuses the unprivileged user, network and pid namespaces that confinement requires",
            ));
        }
        Ok(())
    }

    pub fn bubblewrap(&self) -> &Path {
        &self.bubblewrap
    }

    /// Build the command that starts `launch` inside the namespaces.
    ///
    /// `--unshare-net` is what makes egress denial a property of the kernel
    /// rather than of a filter: the child's network namespace holds a loopback
    /// device and nothing else, so no host address, proxy, or provider
    /// endpoint is routable from it. `--as-pid-1` plus `--die-with-parent`
    /// makes the process tree complete: every descendant, detached or not,
    /// lives in the pid namespace whose init is this child, and the kernel
    /// ends the namespace when that init goes. §FS-rhei-budgets.6.4 §FS-rhei-run.3.2
    pub fn command(&self, launch: &ConfinedLaunch) -> Result<Command> {
        for path in std::iter::once(&launch.program).chain(&launch.read_only) {
            if !path.is_absolute() {
                return Err(missing("a confined read-only path must be absolute"));
            }
        }
        if !launch.work.is_absolute() {
            return Err(missing("the confined work directory must be absolute"));
        }
        let mut cmd = Command::new(&self.bubblewrap);
        cmd.env_clear()
            .args(["--unshare-user", "--unshare-pid", "--unshare-net", "--unshare-ipc"])
            .args(["--unshare-uts", "--disable-userns", "--die-with-parent", "--new-session"])
            .args(["--as-pid-1", "--cap-drop", "ALL", "--clearenv"])
            .args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
        for path in std::iter::once(&launch.program).chain(&launch.read_only) {
            cmd.arg("--ro-bind").arg(path).arg(path);
        }
        cmd.arg("--bind").arg(&launch.work).arg(&launch.work);
        if let Some(socket) = &launch.broker_socket {
            cmd.arg("--bind").arg(socket).arg(BROKER_MOUNT);
        }
        for (key, value) in &launch.environment {
            if key.is_empty() || key.contains('=') || value.contains('\0') {
                return Err(missing("a confined environment entry is not representable"));
            }
            cmd.arg("--setenv").arg(key).arg(value);
        }
        cmd.arg("--setenv").arg("HOME").arg(&launch.work);
        cmd.arg("--chdir").arg(&launch.work);
        cmd.arg("--").arg(&launch.program).args(&launch.argv);
        Ok(cmd)
    }
}

#[cfg(test)]
#[path = "confine_linux_tests.rs"]
mod tests;
