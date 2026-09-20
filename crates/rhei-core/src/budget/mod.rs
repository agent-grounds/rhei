//! Persistent admission primitives shared by CLI and embedded runtimes.
//! §AR-neural-admission.2

mod admission;
mod authority;
mod broker;
mod broker_runtime;
#[cfg(target_os = "linux")]
mod confine_linux;
mod events;
mod forwarding;
mod identity;
mod invalidation;
mod journal;
mod probe;
mod probe_finality;
#[cfg(target_os = "linux")]
mod probe_image;
#[cfg(target_os = "linux")]
mod probe_linux;
mod provider_evidence;
mod qualification;
mod qualification_bundle;
mod recovery;
mod replay;
mod settlement;
mod settlement_inbox;
mod types;

pub use admission::{AdmissionRequest, Arm, ReservationGroup};
pub use broker::{Broker, RequestPermit};
#[cfg(target_os = "linux")]
pub use confine_linux::{ConfinedLaunch, LinuxConfinement, BROKER_MOUNT};
pub use events::{BudgetEvent, BudgetEventSink};
pub use forwarding::Forwarder;
pub use journal::{Audit, Journal, Receipt};
pub use probe::{
    authorize_probe, run_qualification_probes, NativeProbeBackend, ProbeAuthorization, ProbeCase,
    ProbeGrant, ProbeLaunchPermit, RunningProbe,
};
pub use probe_finality::collect_probe_finality;
#[cfg(target_os = "linux")]
pub use probe_linux::LinuxProbeBackend;
pub use provider_evidence::{
    EvidenceRecord, ProviderEvidence, ProviderEvidenceAuthority, VerifiedHistory,
    VerifiedReconciliation,
};
pub use qualification::{LaunchTuple, ObservedLaunch, QualifiedLaunch, Registry};
pub use qualification_bundle::{
    inspect_qualification_bundle, BundleInspection, EvidenceArtifact, EvidenceRole,
    QualificationBundle, RequestBound,
};
pub use settlement::ProviderSettlement;
pub use types::{Allowance, BudgetError, Counter, Money, Snapshot};

pub(crate) type Result<T> = std::result::Result<T, BudgetError>;

/// A raw shell is neural-capable until a confined execution path proves
/// otherwise. This check has no environment-variable exemption.
/// §FS-rhei-budgets.4 §FS-rhei-budgets.6.4
pub fn refuse_unqualified_spawn() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "missing qualification: no verified credential/egress confinement and provider-spend proof; neural work was not started",
    ))
}
