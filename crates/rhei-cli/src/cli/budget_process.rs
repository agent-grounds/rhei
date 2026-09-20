// Process capability transfer is explicit. Production raw launches remain
// refused until native confinement is implemented and qualified.
// §AR-neural-admission.1 §AR-neural-admission.8

struct BudgetProcessCapability {
    #[cfg(feature = "budget-fixtures")]
    forwarder: rhei_core::budget::Forwarder,
    #[cfg(feature = "budget-fixtures")]
    pid: Arc<std::sync::atomic::AtomicU32>,
    #[cfg(feature = "budget-fixtures")]
    stopped: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "budget-fixtures")]
    model: String,
}

impl BudgetLease {
    #[cfg(feature = "budget-fixtures")]
    fn fixture_broker(&self) -> MietteResult<rhei_core::budget::Broker> {
        rhei_core::budget::Broker::open(
            self.root.clone(),
            self.project_uuid.clone(),
            self.reservation_id.clone(),
            self.qualification.clone(),
        )
        .map(|broker| broker.with_event_sink(Arc::new(RuntimeBudgetEventSink(self.sink.clone()))))
        .map_err(budget_error)
    }

    fn prepare_process(&self) -> MietteResult<BudgetProcessCapability> {
        #[cfg(feature = "budget-fixtures")]
        if budget_fixture_active() {
            let pid = Arc::new(std::sync::atomic::AtomicU32::new(0));
            let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let target = pid.clone();
            let stop = stopped.clone();
            let path =
                self.root.join(".agent-grounds/rhei/qualifications/rhei-test-fixed-2000.json");
            let document: FixtureQualificationDocument =
                serde_json::from_slice(&fs::read(&path).map_err(|e| {
                    file_io_report(&path, "cannot read synthetic provider behavior", e)
                })?)
                .map_err(|e| miette!(help = "only the isolated fixture driver reads this document; regenerate it from the test support helpers", "invalid synthetic provider behavior: {e}"))?;
            let forwarder = self
                .fixture_broker()?
                .start_fixture_forwarder(
                    Arc::new(move || {
                        stop.store(true, std::sync::atomic::Ordering::Release);
                        stop_fixture_process(target.load(std::sync::atomic::Ordering::Acquire));
                    }),
                    document.missing_usage,
                )
                .map_err(budget_error)?;
            return Ok(BudgetProcessCapability {
                forwarder,
                pid,
                stopped,
                model: self.qualification.tuple().model.clone(),
            });
        }
        Err(budget_error(rhei_core::budget::BudgetError {
            reason_code: "missing_qualification".into(),
            message: native_confinement_gap(),
        }))
    }
}

/// What is actually missing before a native launch could be handed a process
/// capability, resolved at the one boundary that may hand one out.
///
/// A host that cannot confine at all and a confinable host with no qualified
/// transport are different problems with different repairs, and an operator
/// reading one halt line should not have to guess which they have.
// §FS-rhei-budgets.6.4 §FS-rhei-budgets.9 §AR-neural-admission.5
#[cfg(target_os = "linux")]
fn native_confinement_gap() -> String {
    static HOST: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    let host = HOST.get_or_init(|| {
        rhei_core::budget::LinuxConfinement::resolve().map(|_| ()).map_err(|error| error.message)
    });
    match host {
        Err(why) => {
            format!("this host cannot confine a launch, so no tuple is qualified on it: {why}")
        }
        Ok(()) => "host confinement is available, but no release tuple carries verified provider billing, residual, and a pinned-client broker transport across the confined boundary".into(),
    }
}

#[cfg(not(target_os = "linux"))]
fn native_confinement_gap() -> String {
    "no credential/egress confinement backend exists for this platform, so no tuple is qualified on it".into()
}

impl BudgetProcessCapability {
    fn authorize(&self) -> std::io::Result<()> {
        #[cfg(feature = "budget-fixtures")]
        if budget_fixture_active() && !self.stopped.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }
        rhei_core::budget::refuse_unqualified_spawn()
    }

    fn configure(&self, cmd: &mut std::process::Command) -> MietteResult<()> {
        #[cfg(feature = "budget-fixtures")]
        {
            cmd.env("RHEI_FIXTURE_BROKER_URL", self.forwarder.endpoint())
                .env("RHEI_FIXTURE_BROKER_TOKEN", self.forwarder.token())
                .env("RHEI_FIXTURE_MODEL", &self.model);
        }
        #[cfg(not(feature = "budget-fixtures"))]
        let _ = cmd;
        self.authorize()
            .map_err(|e| miette!(help = native_confinement_gap(), "budget admission halted: {e}"))
    }

    fn attach(&self, child: u32) {
        #[cfg(feature = "budget-fixtures")]
        {
            self.pid.store(child, std::sync::atomic::Ordering::Release);
            if self.stopped.load(std::sync::atomic::Ordering::Acquire) {
                stop_fixture_process(child);
            }
        }
        #[cfg(not(feature = "budget-fixtures"))]
        let _ = child;
    }
}

// Fixture process-group teardown is not a native confinement qualification:
// escaped descendants still require the missing platform backend.
// §FS-rhei-budgets.6.4
#[cfg(feature = "budget-fixtures")]
fn stop_fixture_process(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    {
        let _ = signal::killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .status();
    }
}
